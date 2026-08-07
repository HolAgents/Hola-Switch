//! Identity 管理 — Bind/Unbind 引擎
//!
//! 核心函数:
//! - bind_identity(): 将 identity 绑定到 target，写入 live config
//! - unbind_identity(): 解绑 target
//! - merge_identity_env(): 在 cc-switch live config 写入路径中合并 identity env vars
//! - get_binding_status(): 获取所有 target 的绑定状态

use crate::database::Database;
use crate::error::AppError;
use crate::identity::adapters::AdapterRegistry;
use crate::identity::credentials::{resolve_credential, CredentialTypeSchema};
use crate::identity::dao::{IdentityBinding, IdentityTarget};
use serde::Serialize;
use serde_json::Value;

/// 绑定结果状态行（供 status 命令使用）
#[derive(Debug, Clone, Serialize)]
pub struct StatusRow {
    pub target_id: String,
    pub target_label: String,
    pub identity_name: Option<String>,
    pub active_credentials: Vec<String>,
    pub live_status: String, // "ok", "partial", "missing", "unbound"
}

impl Database {
    /// 将 identity 绑定到 target
    ///
    /// 1. 验证 identity 存在
    /// 2. 解析 identity 的所有 credential
    /// 3. 检查 adapter 接受的 credential 类型
    /// 4. 调用 adapter.bind() 写入 live config
    /// 5. 写入 bindings 表
    /// 6. 记录审计日志
    pub fn identity_bind(
        &self,
        identity_name: &str,
        target_id: &str,
    ) -> Result<(), AppError> {
        // 1. 查找 identity
        let identity = self
            .identity_get_by_name(identity_name)?
            .ok_or_else(|| AppError::Config(format!("Identity '{identity_name}' 不存在")))?;

        // 2. 查找 target
        let target = self
            .identity_target_get(target_id)?
            .ok_or_else(|| AppError::Config(format!("Target '{target_id}' 不存在")))?;

        if !target.enabled {
            return Err(AppError::Config(format!("Target '{target_id}' 已禁用")));
        }

        // 3. 获取 adapter
        let adapter = AdapterRegistry::get(&target.adapter)?;
        let accepted = adapter.accepted_types();

        // 4. 解析 identity 的 credentials
        let db_creds = self.identity_credentials_list(&identity.id)?;
        if db_creds.is_empty() {
            return Err(AppError::Config(format!(
                "Identity '{identity_name}' 没有任何 credential，请先添加"
            )));
        }

        // 检查是否有 adapter 接受的 credential 类型
        let has_accepted = db_creds
            .iter()
            .any(|c| accepted.contains(&c.credential_type.as_str()));
        if !has_accepted {
            return Err(AppError::Config(format!(
                "Identity '{identity_name}' 没有 target '{target_id}' 接受的 credential 类型。\
                 接受的类型: {:?}",
                accepted
            )));
        }

        // 5. 解析每个 credential
        let mut resolved = std::collections::HashMap::new();
        for cred in &db_creds {
            // 获取 credential type schema
            let type_def = self
                .credential_type_get(&cred.credential_type)?
                .ok_or_else(|| {
                    AppError::Config(format!(
                        "未知的 credential 类型: {}",
                        cred.credential_type
                    ))
                })?;

            let schema = CredentialTypeSchema::from_db_row(
                &type_def.id,
                &type_def.name,
                &type_def.schema,
            )?;

            let resolved_cred =
                resolve_credential(&identity.id, &schema, &cred.data)?;
            resolved.insert(cred.credential_type.clone(), resolved_cred);
        }

        // 6. 调用 adapter.bind()
        adapter.bind(&target, identity_name, &resolved)?;

        // 7. 先解绑该 identity 的所有已有绑定（一 identity 一 target）
        self.identity_binding_delete_by_identity(&identity.id)?;

        // 8. 写入新的绑定关系
        self.identity_binding_upsert(&identity.id, target_id)?;

        // 8. 审计日志
        self.identity_audit_insert(
            "bind",
            Some(&identity.id),
            Some(target_id),
            &serde_json::json!({"identity_name": identity_name}).to_string(),
            "success",
        )?;

        log::info!(
            "Bound identity '{}' ({}) to target '{}'",
            identity_name,
            identity.id,
            target_id
        );
        Ok(())
    }

    /// 解绑 target
    pub fn identity_unbind(&self, target_id: &str) -> Result<(), AppError> {
        // 1. 查找当前绑定
        let binding = self.identity_binding_get_by_target(target_id)?;

        // 2. 查找 target
        let target = self
            .identity_target_get(target_id)?
            .ok_or_else(|| AppError::Config(format!("Target '{target_id}' 不存在")))?;

        // 3. 调用 adapter.unbind()
        let adapter = AdapterRegistry::get(&target.adapter)?;
        adapter.unbind(&target)?;

        // 4. 删除绑定关系
        let deleted = self.identity_binding_delete(target_id)?;

        // 5. 审计日志
        let identity_id = binding.as_ref().map(|b| b.identity_id.clone());
        self.identity_audit_insert(
            "unbind",
            identity_id.as_deref(),
            Some(target_id),
            "{}",
            if deleted { "success" } else { "not_found" },
        )?;

        log::info!("Unbound target '{}'", target_id);
        Ok(())
    }

    /// 获取所有 target 的绑定状态（含 live config 实际状态检测）
    pub fn identity_status(&self) -> Result<Vec<StatusRow>, AppError> {
        let targets = self.identity_target_list_enabled()?;
        let mut rows = Vec::new();

        for target in &targets {
            let binding = self.identity_binding_get_by_target(&target.id)?;

            let (identity_name, active_credentials, live_status) = match &binding {
                Some(b) => {
                    // 获取 identity 信息
                    let identity = self.identity_get_by_id(&b.identity_id)?;
                    let name = identity.as_ref().map(|i| i.name.clone());

                    // 获取 credential 类型列表
                    let creds = self
                        .identity_credentials_list(&b.identity_id)
                        .unwrap_or_default();
                    let cred_types: Vec<String> =
                        creds.iter().map(|c| c.credential_type.clone()).collect();

                    // 检测 live config 状态
                    let adapter = AdapterRegistry::get(&target.adapter).ok();
                    let live_status = match adapter {
                        Some(ref a) => match a.is_bound(target) {
                            Ok(state) => {
                                if state.complete {
                                    "ok".to_string()
                                } else if !state.keys_present.is_empty() {
                                    "partial".to_string()
                                } else {
                                    "missing".to_string()
                                }
                            }
                            Err(_) => "unknown".to_string(),
                        },
                        None => "unknown".to_string(),
                    };

                    (name, cred_types, live_status)
                }
                None => (None, Vec::new(), "unbound".to_string()),
            };

            rows.push(StatusRow {
                target_id: target.id.clone(),
                target_label: target.label.clone(),
                identity_name,
                active_credentials,
                live_status,
            });
        }

        Ok(rows)
    }
}

// ============================================================================
// Live Config 集成 — merge_identity_env()
// ============================================================================

/// 在 cc-switch 写出 live config 前，合并 identity 的 env vars
///
/// 此函数被 `services/provider/live.rs` 的 `write_live_snapshot()` 调用。
///
/// - 查询 target_id 的当前绑定
/// - 解析 identity 的 github-account credential
/// - 将 GITHUB_TOKEN, GIT_AUTHOR_* 等注入 settings["env"]
/// - 不覆盖 cc-switch provider 自己的 env key (ANTHROPIC_*, CLAUDE_CODE_* 等)
///
/// 失败时仅 log warning，不影响 cc-switch 正常的 provider 切换。
pub fn merge_identity_env(
    db: &Database,
    target_id: &str,
    settings: &mut Value,
) -> Result<(), AppError> {
    // 查询绑定
    let binding = match db.identity_binding_get_by_target(target_id)? {
        Some(b) => b,
        None => return Ok(()), // 无绑定，无需合并
    };

    // 获取 identity
    let identity = match db.identity_get_by_id(&binding.identity_id)? {
        Some(i) => i,
        None => return Ok(()),
    };

    // 获取 github-account credential
    let cred = match db.identity_credential_get(&identity.id, "github-account")? {
        Some(c) => c,
        None => return Ok(()), // 无 github-account，跳过
    };

    // 获取 credential type schema
    let type_def = match db.credential_type_get("github-account")? {
        Some(t) => t,
        None => return Ok(()),
    };

    let schema = match CredentialTypeSchema::from_db_row(&type_def.id, &type_def.name, &type_def.schema)
    {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };

    // 解析 credential（含从文件系统加载 secret）
    let resolved = match resolve_credential(&identity.id, &schema, &cred.data) {
        Ok(r) => r,
        Err(e) => {
            log::warn!(
                "Failed to resolve credential for identity '{}': {e}",
                identity.name
            );
            return Ok(());
        }
    };

    // 合并到 settings["env"]
    let token = resolved.fields.get("token").map(|s| s.as_str()).unwrap_or("");
    let git_name = resolved.fields.get("git_name").map(|s| s.as_str()).unwrap_or("");
    let git_email = resolved.fields.get("git_email").map(|s| s.as_str()).unwrap_or("");

    if token.is_empty() && git_name.is_empty() && git_email.is_empty() {
        return Ok(());
    }

    // 确保 env 对象存在
    if settings.get("env").is_none() || !settings["env"].is_object() {
        settings["env"] = serde_json::json!({});
    }

    let env = settings["env"]
        .as_object_mut()
        .expect("just ensured env is object");

    // 只添加 identity env key，不覆盖 cc-switch 自己的 ANTHROPIC_* 等 key
    if !token.is_empty() && !env.contains_key("GITHUB_TOKEN") {
        env.insert("GITHUB_TOKEN".to_string(), Value::String(token.to_string()));
        env.insert("GH_TOKEN".to_string(), Value::String(token.to_string()));
    }
    if !git_name.is_empty() && !env.contains_key("GIT_AUTHOR_NAME") {
        env.insert(
            "GIT_AUTHOR_NAME".to_string(),
            Value::String(git_name.to_string()),
        );
        env.insert(
            "GIT_COMMITTER_NAME".to_string(),
            Value::String(git_name.to_string()),
        );
    }
    if !git_email.is_empty() && !env.contains_key("GIT_AUTHOR_EMAIL") {
        env.insert(
            "GIT_AUTHOR_EMAIL".to_string(),
            Value::String(git_email.to_string()),
        );
        env.insert(
            "GIT_COMMITTER_EMAIL".to_string(),
            Value::String(git_email.to_string()),
        );
    }

    log::debug!(
        "Merged identity env vars for target '{}' (identity: '{}')",
        target_id,
        identity.name
    );
    Ok(())
}
