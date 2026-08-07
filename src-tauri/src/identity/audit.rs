//! Identity 管理 — 审计日志服务
//!
//! 所有 mutating 操作（create, delete, add-credential, remove-credential,
//! bind, unbind）自动记录到 identity_audit_log 表。

use crate::database::Database;
use crate::error::AppError;
use crate::identity::dao::AuditEntry;

impl Database {
    /// 记录审计事件
    pub fn identity_audit_log(
        &self,
        action: &str,
        identity_id: Option<&str>,
        target_id: Option<&str>,
        details: &str,
    ) -> Result<(), AppError> {
        self.identity_audit_insert(action, identity_id, target_id, details, "success")
    }

    /// 记录失败的审计事件
    pub fn identity_audit_log_error(
        &self,
        action: &str,
        identity_id: Option<&str>,
        target_id: Option<&str>,
        details: &str,
    ) -> Result<(), AppError> {
        self.identity_audit_insert(action, identity_id, target_id, details, "error")
    }

    /// 查询审计日志
    pub fn identity_audit_query(
        &self,
        identity_name: Option<&str>,
        target_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<AuditEntry>, AppError> {
        // 如果提供了 identity_name，先查询 identity_id
        let identity_id = match identity_name {
            Some(name) => self
                .identity_get_by_name(name)?
                .map(|i| i.id),
            None => None,
        };

        self.identity_audit_list(
            identity_id.as_deref(),
            target_id,
            limit,
        )
    }
}
