//! Identity 管理 — 数据库 Schema 迁移 (v16 → v17)
//!
//! 新增 6 张表:
//! - credential_types: 凭证类型注册表
//! - identities: 身份
//! - identity_credentials: 身份包含的凭证
//! - identity_targets: Target 注册表
//! - identity_bindings: 绑定关系
//! - identity_audit_log: 审计日志

use crate::error::AppError;
use rusqlite::Connection;

/// v16 -> v17: 创建 Identity 管理相关表（含种子数据）
pub(crate) fn migrate_v16_to_v17(conn: &Connection) -> Result<(), AppError> {
    log::info!("迁移数据库从 v16 到 v17（Identity 管理模块）");

    // 1. Credential 类型注册表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS credential_types (
            id      TEXT PRIMARY KEY,
            name    TEXT NOT NULL,
            schema  TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 credential_types 表失败: {e}")))?;

    // 2. Identity 表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS identities (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            description TEXT DEFAULT '',
            created_at  TEXT DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identities 表失败: {e}")))?;

    // 3. Identity 包含的 Credential
    conn.execute(
        "CREATE TABLE IF NOT EXISTS identity_credentials (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            identity_id     TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
            credential_type TEXT NOT NULL REFERENCES credential_types(id),
            data            TEXT NOT NULL,
            created_at      TEXT DEFAULT (datetime('now')),
            UNIQUE(identity_id, credential_type)
        )",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identity_credentials 表失败: {e}")))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_identity_credentials_identity
         ON identity_credentials(identity_id)",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identity_credentials 索引失败: {e}")))?;

    // 4. Target 注册表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS identity_targets (
            id       TEXT PRIMARY KEY,
            adapter  TEXT NOT NULL,
            label    TEXT NOT NULL,
            config   TEXT DEFAULT '{}',
            enabled  INTEGER DEFAULT 1
        )",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identity_targets 表失败: {e}")))?;

    // 5. 绑定关系
    conn.execute(
        "CREATE TABLE IF NOT EXISTS identity_bindings (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
            target_id   TEXT NOT NULL REFERENCES identity_targets(id) ON DELETE CASCADE UNIQUE,
            bound_at    TEXT DEFAULT (datetime('now'))
        )",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identity_bindings 表失败: {e}")))?;

    // 6. 审计日志
    conn.execute(
        "CREATE TABLE IF NOT EXISTS identity_audit_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   TEXT DEFAULT (datetime('now')),
            action      TEXT NOT NULL,
            identity_id TEXT,
            target_id   TEXT,
            details     TEXT DEFAULT '{}',
            result      TEXT DEFAULT 'success'
        )",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identity_audit_log 表失败: {e}")))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_identity_audit_log_timestamp
         ON identity_audit_log(timestamp)",
        [],
    )
    .map_err(|e| AppError::Database(format!("创建 identity_audit_log 索引失败: {e}")))?;

    // 7. 种子数据
    seed_identity_data(conn)?;

    log::info!("v16 -> v17 迁移完成：Identity 管理模块已就绪");
    Ok(())
}

/// 插入 credential_types 和 identity_targets 的初始种子数据
fn seed_identity_data(conn: &Connection) -> Result<(), AppError> {
    // Credential 类型
    conn.execute(
        "INSERT OR IGNORE INTO credential_types (id, name, schema) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            "github-account",
            "GitHub Account",
            r#"{"fields":{"token":{"secret":true,"env_key":"GITHUB_TOKEN"},"git_name":{"secret":false,"env_key":"GIT_AUTHOR_NAME"},"git_email":{"secret":false,"env_key":"GIT_AUTHOR_EMAIL"},"ssh_key_path":{"secret":false,"optional":true}}}"#
        ],
    )
    .map_err(|e| AppError::Database(format!("插入 credential_type github-account 失败: {e}")))?;

    // Target 注册
    let targets = [
        ("claude-code", "claude-code", "Claude Code CLI", "{}"),
        ("hermes:default", "hermes", "Hermes (default)", r#"{"profile":"default","hermes_home":"~/.hermes"}"#),
        ("hermes:ops", "hermes", "Hermes (ops)", r#"{"profile":"ops","hermes_home":"~/.hermes/profiles/ops"}"#),
        ("hermes:po", "hermes", "Hermes (po)", r#"{"profile":"po","hermes_home":"~/.hermes/profiles/po"}"#),
    ];

    for (id, adapter, label, config) in &targets {
        conn.execute(
            "INSERT OR IGNORE INTO identity_targets (id, adapter, label, config) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, adapter, label, config],
        )
        .map_err(|e| AppError::Database(format!("插入 identity_target {id} 失败: {e}")))?;
    }

    Ok(())
}
