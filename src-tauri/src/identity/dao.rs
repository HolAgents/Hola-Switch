//! Identity 管理 — 数据访问对象 (DAO)
//!
//! 提供 identities, identity_credentials, identity_bindings,
//! identity_targets, identity_audit_log 的 CRUD 操作。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use rusqlite::params;
use serde::{Deserialize, Serialize};

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCredential {
    pub id: i64,
    pub identity_id: String,
    pub credential_type: String,
    /// JSON: 非敏感字段存原值, 敏感字段存 "sha256:<hex>" 指纹
    pub data: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityTarget {
    pub id: String,
    pub adapter: String,
    pub label: String,
    pub config: String, // JSON
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityBinding {
    pub id: i64,
    pub identity_id: String,
    pub target_id: String,
    pub bound_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub timestamp: String,
    pub action: String,
    pub identity_id: Option<String>,
    pub target_id: Option<String>,
    pub details: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialTypeDef {
    pub id: String,
    pub name: String,
    pub schema: String, // JSON
}

// ============================================================================
// Database methods — Identity CRUD
// ============================================================================

impl Database {
    // ---- Identities ----

    pub fn identity_insert(&self, id: &str, name: &str, description: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO identities (id, name, description) VALUES (?1, ?2, ?3)",
            params![id, name, description],
        )
        .map_err(|e| AppError::Database(format!("创建 identity 失败: {e}")))?;
        Ok(())
    }

    pub fn identity_get_by_name(&self, name: &str) -> Result<Option<Identity>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, name, description, created_at FROM identities WHERE name = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let result = stmt
            .query_row(params![name], |row| {
                Ok(Identity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .ok();
        Ok(result)
    }

    pub fn identity_get_by_id(&self, id: &str) -> Result<Option<Identity>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, name, description, created_at FROM identities WHERE id = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let result = stmt
            .query_row(params![id], |row| {
                Ok(Identity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .ok();
        Ok(result)
    }

    pub fn identity_list(&self) -> Result<Vec<Identity>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, name, description, created_at FROM identities ORDER BY created_at DESC")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Identity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn identity_delete_by_id(&self, id: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute("DELETE FROM identities WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(format!("删除 identity 失败: {e}")))?;
        Ok(affected > 0)
    }

    // ---- Identity Credentials ----

    pub fn identity_credential_insert(
        &self,
        identity_id: &str,
        credential_type: &str,
        data: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO identity_credentials (identity_id, credential_type, data)
             VALUES (?1, ?2, ?3)",
            params![identity_id, credential_type, data],
        )
        .map_err(|e| AppError::Database(format!("添加 credential 失败: {e}")))?;
        Ok(())
    }

    pub fn identity_credential_get(
        &self,
        identity_id: &str,
        credential_type: &str,
    ) -> Result<Option<IdentityCredential>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, identity_id, credential_type, data, created_at
                 FROM identity_credentials
                 WHERE identity_id = ?1 AND credential_type = ?2",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let result = stmt
            .query_row(params![identity_id, credential_type], |row| {
                Ok(IdentityCredential {
                    id: row.get(0)?,
                    identity_id: row.get(1)?,
                    credential_type: row.get(2)?,
                    data: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .ok();
        Ok(result)
    }

    pub fn identity_credentials_list(
        &self,
        identity_id: &str,
    ) -> Result<Vec<IdentityCredential>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, identity_id, credential_type, data, created_at
                 FROM identity_credentials
                 WHERE identity_id = ?1
                 ORDER BY credential_type",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![identity_id], |row| {
                Ok(IdentityCredential {
                    id: row.get(0)?,
                    identity_id: row.get(1)?,
                    credential_type: row.get(2)?,
                    data: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn identity_credential_delete(
        &self,
        identity_id: &str,
        credential_type: &str,
    ) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "DELETE FROM identity_credentials
                 WHERE identity_id = ?1 AND credential_type = ?2",
                params![identity_id, credential_type],
            )
            .map_err(|e| AppError::Database(format!("删除 credential 失败: {e}")))?;
        Ok(affected > 0)
    }

    // ---- Target Registry ----

    pub fn identity_target_get(&self, id: &str) -> Result<Option<IdentityTarget>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, adapter, label, config, enabled FROM identity_targets WHERE id = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let result = stmt
            .query_row(params![id], |row| {
                Ok(IdentityTarget {
                    id: row.get(0)?,
                    adapter: row.get(1)?,
                    label: row.get(2)?,
                    config: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? != 0,
                })
            })
            .ok();
        Ok(result)
    }

    pub fn identity_target_list_enabled(&self) -> Result<Vec<IdentityTarget>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, adapter, label, config, enabled
                 FROM identity_targets WHERE enabled = 1 ORDER BY id",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(IdentityTarget {
                    id: row.get(0)?,
                    adapter: row.get(1)?,
                    label: row.get(2)?,
                    config: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? != 0,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    // ---- Bindings ----

    pub fn identity_binding_get_by_target(
        &self,
        target_id: &str,
    ) -> Result<Option<IdentityBinding>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, identity_id, target_id, bound_at
                 FROM identity_bindings WHERE target_id = ?1",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let result = stmt
            .query_row(params![target_id], |row| {
                Ok(IdentityBinding {
                    id: row.get(0)?,
                    identity_id: row.get(1)?,
                    target_id: row.get(2)?,
                    bound_at: row.get(3)?,
                })
            })
            .ok();
        Ok(result)
    }

    pub fn identity_binding_get_by_identity(
        &self,
        identity_id: &str,
    ) -> Result<Vec<IdentityBinding>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, identity_id, target_id, bound_at
                 FROM identity_bindings WHERE identity_id = ?1",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![identity_id], |row| {
                Ok(IdentityBinding {
                    id: row.get(0)?,
                    identity_id: row.get(1)?,
                    target_id: row.get(2)?,
                    bound_at: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn identity_binding_list(&self) -> Result<Vec<IdentityBinding>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, identity_id, target_id, bound_at
                 FROM identity_bindings ORDER BY target_id",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(IdentityBinding {
                    id: row.get(0)?,
                    identity_id: row.get(1)?,
                    target_id: row.get(2)?,
                    bound_at: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    pub fn identity_binding_upsert(
        &self,
        identity_id: &str,
        target_id: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO identity_bindings (identity_id, target_id, bound_at)
             VALUES (?1, ?2, datetime('now'))",
            params![identity_id, target_id],
        )
        .map_err(|e| AppError::Database(format!("创建/更新绑定失败: {e}")))?;
        Ok(())
    }

    pub fn identity_binding_delete(&self, target_id: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "DELETE FROM identity_bindings WHERE target_id = ?1",
                params![target_id],
            )
            .map_err(|e| AppError::Database(format!("删除绑定失败: {e}")))?;
        Ok(affected > 0)
    }

    /// 删除某个 identity 的所有绑定（一 identity 一 target 约束）
    pub fn identity_binding_delete_by_identity(&self, identity_id: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "DELETE FROM identity_bindings WHERE identity_id = ?1",
                params![identity_id],
            )
            .map_err(|e| AppError::Database(format!("删除绑定失败: {e}")))?;
        Ok(affected > 0)
    }

    // ---- Audit Log ----

    pub fn identity_audit_insert(
        &self,
        action: &str,
        identity_id: Option<&str>,
        target_id: Option<&str>,
        details: &str,
        result: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT INTO identity_audit_log (action, identity_id, target_id, details, result)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![action, identity_id, target_id, details, result],
        )
        .map_err(|e| AppError::Database(format!("写入审计日志失败: {e}")))?;
        Ok(())
    }

    pub fn identity_audit_list(
        &self,
        identity_id: Option<&str>,
        target_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<AuditEntry>, AppError> {
        let conn = lock_conn!(self.conn);
        let limit_val = limit.unwrap_or(50) as i64;

        // Build SQL dynamically based on filters
        let mut sql = String::from(
            "SELECT id, timestamp, action, identity_id, target_id, details, result
             FROM identity_audit_log"
        );
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(id) = identity_id {
            conditions.push(format!("identity_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(id.to_string()));
        }
        if let Some(tid) = target_id {
            conditions.push(format!("target_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(tid.to_string()));
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(&format!(
            " ORDER BY id DESC LIMIT ?{}",
            param_values.len() + 1
        ));
        param_values.push(Box::new(limit_val));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(AuditEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    action: row.get(2)?,
                    identity_id: row.get(3)?,
                    target_id: row.get(4)?,
                    details: row.get(5)?,
                    result: row.get(6)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(result)
    }

    // ---- Credential Types ----

    pub fn credential_type_get(&self, id: &str) -> Result<Option<CredentialTypeDef>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, name, schema FROM credential_types WHERE id = ?1")
            .map_err(|e| AppError::Database(e.to_string()))?;
        let result = stmt
            .query_row(params![id], |row| {
                Ok(CredentialTypeDef {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    schema: row.get(2)?,
                })
            })
            .ok();
        Ok(result)
    }
}
