//! Identity 管理模块
//!
//! 管理 Agent Identity（开发者身份）的创建、credential 关联、以及
//! 将 identity 绑定/注入到 AI agent 框架（Claude Code, Hermes）的配置文件中。
//!
//! ## 模块结构
//!
//! ```text
//! identity/
//! ├── mod.rs           — 模块入口 + IdentityService
//! ├── schema.rs        — DB migration v16→v17
//! ├── dao.rs           — CRUD: identities, credentials, bindings, audit
//! ├── credentials.rs   — Credential 类型注册表 + 敏感值文件存储
//! ├── binder.rs        — bind/unbind 引擎 + merge_identity_env()
//! ├── audit.rs         — 审计日志服务
//! └── adapters/
//!     ├── mod.rs       — TargetAdapter trait + registry
//!     ├── claude_code.rs — Claude Code settings.json adapter
//!     └── hermes.rs    — Hermes profile .env adapter
//! ```
//!
//! ## 核心概念
//!
//! - **Credential** = 凭证类型实例（github-account = PAT + git_name + git_email）
//! - **Identity** = 命名 credential 组合（"holagent001" = github-account）
//! - **Target** = AI 框架实例（claude-code, hermes:default, hermes:ops）
//! - **Bind** = Identity → Target 的持久映射，写入 Target 配置文件
//!
//! ## 安全设计
//!
//! - 敏感值（token）存储在文件系统 `credentials/{identity_id}/{type}.env`（0600）
//! - DB 中仅存储 sha256 指纹，备份和导出不泄露 token
//! - 所有 mutating 操作自动记录审计日志

pub mod adapters;
pub mod audit;
pub mod binder;
pub mod credentials;
pub mod dao;
pub mod schema;

#[cfg(test)]
mod tests;

use crate::database::Database;
use crate::error::AppError;
use crate::identity::credentials::{
    build_credential_data_json, delete_secret_file, validate_credential_fields,
    write_secret_file, CredentialTypeSchema,
};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

/// credential_add_github 的返回结果，包含 SSH key 自动生成信息
#[derive(Debug, Clone, Serialize)]
pub struct CredentialAddResult {
    pub ssh_key_path: String,
    pub ssh_key_uploaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_public_key: Option<String>,
    pub ssh_setup_url: String,
}

/// Identity 管理服务
///
/// 封装 identity 和 credential 的创建/删除/查询逻辑。
/// 绑定/解绑操作在 `binder.rs` 中通过 `Database` 扩展方法实现。
impl Database {
    /// 创建 identity
    ///
    /// 自动生成 UUID 作为 identity_id，name 必须唯一。
    pub fn identity_create(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<String, AppError> {
        // 检查名称是否已存在
        if self.identity_get_by_name(name)?.is_some() {
            return Err(AppError::Config(format!(
                "Identity '{name}' 已存在"
            )));
        }

        let id = Uuid::new_v4().to_string();
        let desc = description.unwrap_or("");

        self.identity_insert(&id, name, desc)?;

        self.identity_audit_log(
            "identity.create",
            Some(&id),
            None,
            &serde_json::json!({"name": name, "description": desc}).to_string(),
        )?;

        log::info!("Created identity '{name}' ({id})");
        Ok(id)
    }

    /// 删除 identity
    ///
    /// - 检查是否有活跃绑定（默认拒绝）
    /// - 删除所有 credential 文件
    /// - 级联删除 DB 中的 identity, credentials, bindings
    pub fn identity_delete(&self, name: &str) -> Result<(), AppError> {
        let identity = self
            .identity_get_by_name(name)?
            .ok_or_else(|| AppError::Config(format!("Identity '{name}' 不存在")))?;

        // 检查是否有活跃绑定
        let bindings = self.identity_binding_get_by_identity(&identity.id)?;
        if !bindings.is_empty() {
            let targets: Vec<String> =
                bindings.iter().map(|b| b.target_id.clone()).collect();
            return Err(AppError::Config(format!(
                "Identity '{name}' 仍有活跃绑定: {targets:?}。请先 unbind 后再删除。"
            )));
        }

        // 收集 credential 类型以便删除文件
        let creds = self.identity_credentials_list(&identity.id)?;

        // 删除 DB 记录（级联删除 bindings 和 credentials）
        self.identity_delete_by_id(&identity.id)?;

        // 删除 secret 文件
        for cred in &creds {
            if let Err(e) = delete_secret_file(&identity.id, &cred.credential_type) {
                log::warn!("Failed to delete secret file: {e}");
            }
        }

        // 清理 identity 目录
        let cred_dir = crate::config::get_app_config_dir()
            .join("credentials")
            .join(&identity.id);
        if cred_dir.exists() {
            let _ = std::fs::remove_dir_all(&cred_dir);
        }

        self.identity_audit_log(
            "identity.delete",
            Some(&identity.id),
            None,
            &serde_json::json!({"name": name}).to_string(),
        )?;

        log::info!("Deleted identity '{name}' ({})", identity.id);
        Ok(())
    }

    /// 为 identity 添加 github-account credential
    ///
    /// 参数:
    /// - identity_name: identity 名称
    /// - token: GitHub Personal Access Token
    /// - git_name: git user.name
    /// - git_email: git user.email
    /// - ssh_key_path: (可选) SSH 私钥路径
    pub fn credential_add_github(
        &self,
        identity_name: &str,
        token: &str,
        git_name: &str,
        git_email: &str,
        ssh_key_path: Option<&str>,
    ) -> Result<CredentialAddResult, AppError> {
        let identity = self
            .identity_get_by_name(identity_name)?
            .ok_or_else(|| AppError::Config(format!("Identity '{identity_name}' 不存在")))?;

        // 获取 credential type schema
        let type_def = self
            .credential_type_get("github-account")?
            .ok_or_else(|| AppError::Config("credential type 'github-account' 未注册".to_string()))?;

        let schema = CredentialTypeSchema::from_db_row(
            &type_def.id,
            &type_def.name,
            &type_def.schema,
        )?;

        // 验证 token
        if token.trim().is_empty() {
            return Err(AppError::Config("GitHub token 不能为空".to_string()));
        }

        // 验证 token 是否有效（调用 GitHub API）
        let validate = std::process::Command::new("gh")
            .args(["api", "user", "--hostname", "github.com"])
            .env("GH_TOKEN", token)
            .env("GITHUB_TOKEN", token)
            .output();
        match validate {
            Ok(o) if o.status.success() => {
                log::info!("GitHub token validated for identity '{}'", identity_name);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                return Err(AppError::Config(format!(
                    "GitHub token 无效: {}",
                    stderr.trim()
                )));
            }
            Err(e) => {
                return Err(AppError::Config(format!(
                    "无法验证 GitHub token (gh CLI 不可用): {e}"
                )));
            }
        }

        // SSH key 自动生成（用户未提供时）
        let home = crate::config::get_home_dir();
        let final_ssh_path: String;
        let mut ssh_public_key: Option<String> = None;
        let mut ssh_uploaded = true;

        if let Some(path) = ssh_key_path.filter(|p| !p.trim().is_empty()) {
            final_ssh_path = path.to_string();
        } else {
            let key_name = format!("{}_ed25519", identity_name);
            let private_key = home.join(".ssh").join(&key_name);
            let public_key = home.join(".ssh").join(format!("{key_name}.pub"));

            // 生成 SSH key（本地操作，不依赖网络）
            if !private_key.exists() {
                let output = std::process::Command::new("ssh-keygen")
                    .args([
                        "-t", "ed25519",
                        "-C", git_email,
                        "-f", &private_key.to_string_lossy(),
                        "-N", "",
                    ])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        log::info!("Generated SSH key for identity '{}': {}", identity_name, private_key.display());
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        log::warn!("ssh-keygen failed for '{}': {}", identity_name, stderr);
                        return Err(AppError::Config(format!("SSH key 生成失败: {stderr}")));
                    }
                    Err(e) => {
                        log::warn!("ssh-keygen not found: {e}");
                        return Err(AppError::Config("ssh-keygen 未找到，请安装 OpenSSH".to_string()));
                    }
                }
            }

            // 尝试上传公钥到 GitHub（best-effort）
            if public_key.exists() {
                ssh_public_key = std::fs::read_to_string(&public_key).ok();
                let upload = std::process::Command::new("gh")
                    .args([
                        "ssh-key", "add",
                        &public_key.to_string_lossy(),
                        "-t", &format!("{identity_name}-cc-switch"),
                    ])
                    .env("GH_TOKEN", token)
                    .output();
                match upload {
                    Ok(o) if o.status.success() => {
                        log::info!("Uploaded SSH key to GitHub for identity '{}'", identity_name);
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        log::warn!("gh ssh-key add failed for '{}': {}", identity_name, stderr);
                        ssh_uploaded = false;
                    }
                    Err(e) => {
                        log::warn!("gh CLI not available for SSH upload: {e}");
                        ssh_uploaded = false;
                    }
                }
            }

            final_ssh_path = format!("~/.ssh/{key_name}");
        }

        // 构建字段映射
        let mut fields = HashMap::new();
        fields.insert("token".to_string(), token.to_string());
        fields.insert("git_name".to_string(), git_name.to_string());
        fields.insert("git_email".to_string(), git_email.to_string());
        fields.insert("ssh_key_path".to_string(), final_ssh_path.clone());

        validate_credential_fields(&schema, &fields)?;

        // 检查是否已存在（需要 --force flag，这里先拒绝）
        if self
            .identity_credential_get(&identity.id, "github-account")?
            .is_some()
        {
            return Err(AppError::Config(
                "该 identity 已有 github-account credential。\
                 如需替换请先 credential remove 后再添加。"
                    .to_string(),
            ));
        }

        // 构建 data JSON（secret 字段存指纹）
        let data_json = build_credential_data_json(&schema, &fields)?;

        // 写入 secret 文件
        let mut secrets = HashMap::new();
        secrets.insert("GITHUB_TOKEN".to_string(), token.to_string());
        write_secret_file(&identity.id, "github-account", &secrets)?;

        // 写入 DB
        self.identity_credential_insert(&identity.id, "github-account", &data_json)?;

        self.identity_audit_log(
            "credential.add",
            Some(&identity.id),
            None,
            &serde_json::json!({"type": "github-account", "git_name": git_name, "git_email": git_email}).to_string(),
        )?;

        log::info!(
            "Added github-account credential to identity '{}'",
            identity_name
        );
        Ok(CredentialAddResult {
            ssh_key_path: final_ssh_path,
            ssh_key_uploaded: ssh_uploaded,
            ssh_public_key: if ssh_uploaded { None } else { ssh_public_key },
            ssh_setup_url: "https://github.com/settings/keys".to_string(),
        })
    }

    /// 移除 identity 的某个 credential
    pub fn credential_remove(
        &self,
        identity_name: &str,
        credential_type: &str,
    ) -> Result<(), AppError> {
        let identity = self
            .identity_get_by_name(identity_name)?
            .ok_or_else(|| AppError::Config(format!("Identity '{identity_name}' 不存在")))?;

        // 检查 credential 是否存在
        let cred = self
            .identity_credential_get(&identity.id, credential_type)?;
        if cred.is_none() {
            return Err(AppError::Config(format!(
                "Identity '{identity_name}' 没有 {credential_type} credential"
            )));
        }

        // 删除 secret 文件
        delete_secret_file(&identity.id, credential_type)?;

        // 删除 DB 记录
        self.identity_credential_delete(&identity.id, credential_type)?;

        self.identity_audit_log(
            "credential.remove",
            Some(&identity.id),
            None,
            &serde_json::json!({"type": credential_type}).to_string(),
        )?;

        log::info!(
            "Removed {credential_type} credential from identity '{identity_name}'"
        );
        Ok(())
    }
}
