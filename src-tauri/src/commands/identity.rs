//! Identity 管理 — Tauri Commands
//!
//! 所有 identity 操作通过 Tauri commands 暴露，供 CLI 和未来 GUI 调用。
//!
//! CLI 模式：通过 `cc-switch identity <subcommand>` 子命令调用。
//! GUI 模式（Phase 2）：React 前端通过 Tauri IPC invoke 调用。

use crate::database::Database;
use crate::identity::CredentialAddResult;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::store::AppState;

// ============================================================================
// Data Types (shared with CLI output)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub credentials: Vec<CredentialInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialInfo {
    pub credential_type: String,
    /// 非敏感字段原值, 敏感字段显示 "sha256:..."
    pub fields: std::collections::HashMap<String, String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusRow {
    pub target_id: String,
    pub target_label: String,
    pub identity_name: Option<String>,
    pub active_credentials: Vec<String>,
    pub live_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRow {
    pub timestamp: String,
    pub action: String,
    pub identity_name: Option<String>,
    pub target_id: Option<String>,
    pub details: String,
    pub result: String,
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// 创建 identity
#[tauri::command]
pub fn identity_create(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<IdentityInfo, String> {
    let db = &state.db;
    let id = db
        .identity_create(&name, description.as_deref())
        .map_err(|e| e.to_string())?;
    identity_show_inner(db, &name)
}

/// 查看 identity 详情
#[tauri::command]
pub fn identity_show(state: State<'_, AppState>, name: String) -> Result<IdentityInfo, String> {
    identity_show_inner(&state.db, &name)
}

/// 列出所有 identity
#[tauri::command]
pub fn identity_list(state: State<'_, AppState>) -> Result<Vec<IdentityInfo>, String> {
    let identities = state
        .db
        .identity_list()
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for iden in identities {
        let creds = state
            .db
            .identity_credentials_list(&iden.id)
            .map_err(|e| e.to_string())?;

        let credential_infos: Vec<CredentialInfo> = creds
            .iter()
            .map(|c| {
                let data: serde_json::Value =
                    serde_json::from_str(&c.data).unwrap_or_default();
                let mut fields = std::collections::HashMap::new();
                if let Some(obj) = data.as_object() {
                    for (k, v) in obj {
                        fields.insert(
                            k.clone(),
                            v.as_str().unwrap_or("").to_string(),
                        );
                    }
                }
                CredentialInfo {
                    credential_type: c.credential_type.clone(),
                    fields,
                    created_at: c.created_at.clone(),
                }
            })
            .collect();

        result.push(IdentityInfo {
            id: iden.id,
            name: iden.name,
            description: iden.description,
            created_at: iden.created_at,
            credentials: credential_infos,
        });
    }

    Ok(result)
}

/// 删除 identity
#[tauri::command]
pub fn identity_delete(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state
        .db
        .identity_delete(&name)
        .map_err(|e| e.to_string())
}

/// 添加 github-account credential
#[tauri::command]
#[allow(non_snake_case)]
pub fn credential_add_github(
    state: State<'_, AppState>,
    identity: String,
    token: String,
    gitName: String,
    gitEmail: String,
    sshKeyPath: Option<String>,
) -> Result<CredentialAddResult, String> {
    state
        .db
        .credential_add_github(
            &identity,
            &token,
            &gitName,
            &gitEmail,
            sshKeyPath.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 移除 credential
#[tauri::command]
pub fn credential_remove(
    state: State<'_, AppState>,
    identity: String,
    credential_type: String,
) -> Result<(), String> {
    state
        .db
        .credential_remove(&identity, &credential_type)
        .map_err(|e| e.to_string())
}

/// 绑定 identity 到 target
#[tauri::command]
pub fn identity_bind(
    state: State<'_, AppState>,
    identity: String,
    target: String,
) -> Result<(), String> {
    state
        .db
        .identity_bind(&identity, &target)
        .map_err(|e| e.to_string())
}

/// 解绑 target
#[tauri::command]
pub fn identity_unbind(
    state: State<'_, AppState>,
    target: String,
) -> Result<(), String> {
    state
        .db
        .identity_unbind(&target)
        .map_err(|e| e.to_string())
}

/// 获取所有 target 的绑定状态
#[tauri::command]
pub fn identity_status(state: State<'_, AppState>) -> Result<Vec<StatusRow>, String> {
    let rows = state
        .db
        .identity_status()
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|r| StatusRow {
            target_id: r.target_id,
            target_label: r.target_label,
            identity_name: r.identity_name,
            active_credentials: r.active_credentials,
            live_status: r.live_status,
        })
        .collect())
}

/// 查询审计日志
#[tauri::command]
pub fn identity_audit(
    state: State<'_, AppState>,
    identity: Option<String>,
    target: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<AuditRow>, String> {
    let entries = state
        .db
        .identity_audit_query(
            identity.as_deref(),
            target.as_deref(),
            limit,
        )
        .map_err(|e| e.to_string())?;

    Ok(entries
        .into_iter()
        .map(|e| AuditRow {
            timestamp: e.timestamp,
            action: e.action,
            identity_name: e.identity_id.and_then(|id| {
                state
                    .db
                    .identity_get_by_id(&id)
                    .ok()
                    .flatten()
                    .map(|i| i.name)
            }),
            target_id: e.target_id,
            details: e.details,
            result: e.result,
        })
        .collect())
}

// ============================================================================
// Internal Helpers
// ============================================================================

fn identity_show_inner(db: &Database, name: &str) -> Result<IdentityInfo, String> {
    let identity = db
        .identity_get_by_name(name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Identity '{name}' 不存在"))?;

    let creds = db
        .identity_credentials_list(&identity.id)
        .map_err(|e| e.to_string())?;

    let credential_infos: Vec<CredentialInfo> = creds
        .iter()
        .map(|c| {
            let data: serde_json::Value =
                serde_json::from_str(&c.data).unwrap_or_default();
            let mut fields = std::collections::HashMap::new();
            if let Some(obj) = data.as_object() {
                for (k, v) in obj {
                    fields.insert(
                        k.clone(),
                        v.as_str().unwrap_or("").to_string(),
                    );
                }
            }
            CredentialInfo {
                credential_type: c.credential_type.clone(),
                fields,
                created_at: c.created_at.clone(),
            }
        })
        .collect();

    Ok(IdentityInfo {
        id: identity.id,
        name: identity.name,
        description: identity.description,
        created_at: identity.created_at,
        credentials: credential_infos,
    })
}
