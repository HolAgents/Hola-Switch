//! Identity 管理 — Claude Code Adapter
//!
//! 管理 `~/.claude/settings.json` 的 `env` 块中的 git identity key。
//!
//! 管理的 key（6 个）:
//! - GITHUB_TOKEN, GH_TOKEN
//! - GIT_AUTHOR_NAME, GIT_AUTHOR_EMAIL
//! - GIT_COMMITTER_NAME, GIT_COMMITTER_EMAIL
//!
//! 行为:
//! - bind(): 读取 settings.json → 在 env 块中 upsert 6 key → 保留所有其他 key → atomic_write
//! - unbind(): 移除 6 key → 若 env 变空则移除整个 env → atomic_write
//! - is_bound(): 检查 live env 中 6 key 是否完整

use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
use crate::error::AppError;
use crate::identity::adapters::{BoundState, TargetAdapter};
use crate::identity::credentials::ResolvedCredential;
use crate::identity::dao::IdentityTarget;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;

/// Identity 管理的 git env key 列表（含旧 key 以便清理）
const MANAGED_ENV_KEYS: &[&str] = &[
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    // 旧版 key（如有）也清理
    "GIT_AUTHOR",
    "GIT_COMMITTER",
];

/// Managed block marker（写在 settings.json 的顶层注释字段中）
const MANAGED_MARKER_KEY: &str = "_idswitch_identity";

pub struct ClaudeCodeAdapter;

impl TargetAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn accepted_types(&self) -> &'static [&'static str] {
        &["github-account"]
    }

    fn bind(
        &self,
        _target: &IdentityTarget,
        identity_name: &str,
        creds: &HashMap<String, ResolvedCredential>,
    ) -> Result<(), AppError> {
        let path = get_claude_settings_path();

        // 读取现有 settings.json（不存在则创建空对象）
        let mut settings: Value = if path.exists() {
            read_json_file(&path).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };

        // 确保 settings 是对象
        if !settings.is_object() {
            settings = json!({});
        }

        // 从 github-account credential 提取字段
        let gh_cred = creds.get("github-account").ok_or_else(|| {
            AppError::Config("缺少 github-account credential".to_string())
        })?;

        let token = gh_cred.fields.get("token").map(|s| s.as_str()).unwrap_or("");
        let git_name = gh_cred.fields.get("git_name").map(|s| s.as_str()).unwrap_or("");
        let git_email = gh_cred.fields.get("git_email").map(|s| s.as_str()).unwrap_or("");

        // 确保 env 对象存在
        if settings.get("env").is_none() || !settings["env"].is_object() {
            settings["env"] = json!({});
        }

        let env = settings["env"].as_object_mut().expect("just ensured");

        // 注入 git identity env vars
        if !token.is_empty() {
            env.insert("GITHUB_TOKEN".to_string(), Value::String(token.to_string()));
            env.insert("GH_TOKEN".to_string(), Value::String(token.to_string()));
        }
        if !git_name.is_empty() {
            env.insert(
                "GIT_AUTHOR_NAME".to_string(),
                Value::String(git_name.to_string()),
            );
            env.insert(
                "GIT_COMMITTER_NAME".to_string(),
                Value::String(git_name.to_string()),
            );
        }
        if !git_email.is_empty() {
            env.insert(
                "GIT_AUTHOR_EMAIL".to_string(),
                Value::String(git_email.to_string()),
            );
            env.insert(
                "GIT_COMMITTER_EMAIL".to_string(),
                Value::String(git_email.to_string()),
            );
        }

        // 标记 managed identity（供 is_bound 和 unbind 识别）
        if let Some(obj) = settings.as_object_mut() {
            obj.insert(
                MANAGED_MARKER_KEY.to_string(),
                Value::String(identity_name.to_string()),
            );
        }

        // 备份原文件
        create_backup(&path)?;

        // 原子写入
        write_json_file(&path, &settings)?;

        log::info!(
            "ClaudeCodeAdapter: bound identity '{}' to {}",
            identity_name,
            path.display()
        );
        Ok(())
    }

    fn unbind(&self, _target: &IdentityTarget) -> Result<(), AppError> {
        let path = get_claude_settings_path();

        if !path.exists() {
            return Ok(());
        }

        let mut settings: Value = read_json_file(&path).unwrap_or_else(|_| json!({}));
        if !settings.is_object() {
            return Ok(());
        }

        // 移除 managed env keys
        if let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) {
            for key in MANAGED_ENV_KEYS {
                env.remove(*key);
            }
            // 如果 env 变空，移除整个 env 对象
            if env.is_empty() {
                if let Some(obj) = settings.as_object_mut() {
                    obj.remove("env");
                }
            }
        }

        // 移除 managed marker
        if let Some(obj) = settings.as_object_mut() {
            obj.remove(MANAGED_MARKER_KEY);
        }

        create_backup(&path)?;
        write_json_file(&path, &settings)?;

        log::info!("ClaudeCodeAdapter: unbound from {}", path.display());
        Ok(())
    }

    fn is_bound(&self, _target: &IdentityTarget) -> Result<BoundState, AppError> {
        let path = get_claude_settings_path();

        if !path.exists() {
            return Ok(BoundState {
                identity_name: None,
                keys_present: Vec::new(),
                complete: false,
            });
        }

        let settings: Value = read_json_file(&path).unwrap_or_else(|_| json!({}));

        let identity_name = settings
            .get(MANAGED_MARKER_KEY)
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let mut keys_present = Vec::new();
        if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
            for key in MANAGED_ENV_KEYS {
                if env.contains_key(*key) {
                    keys_present.push(key.to_string());
                }
            }
        }

        // 核心 6 key: GITHUB_TOKEN, GH_TOKEN, GIT_AUTHOR_NAME, GIT_AUTHOR_EMAIL,
        //              GIT_COMMITTER_NAME, GIT_COMMITTER_EMAIL
        let core_keys: &[&str] = &[
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
        ];
        let complete = core_keys.iter().all(|k| keys_present.contains(&k.to_string()));

        Ok(BoundState {
            identity_name,
            keys_present,
            complete,
        })
    }
}

/// 创建配置文件备份
fn create_backup(path: &std::path::Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    let backup_dir = crate::config::get_app_config_dir()
        .join("backups")
        .join("claude-code");
    fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "settings.json".to_string());
    let backup_path = backup_dir.join(format!("{filename}.{ts}.bak"));

    fs::copy(path, &backup_path).map_err(|e| AppError::io(&backup_path, e))?;

    // 保留最近 10 个备份
    prune_backups(&backup_dir, &filename, 10)?;

    Ok(())
}

/// 清理旧备份（保留最近 N 个）
fn prune_backups(dir: &std::path::Path, prefix: &str, keep: usize) -> Result<(), AppError> {
    let mut backups: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| AppError::io(dir, e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(prefix)
        })
        .collect();

    if backups.len() <= keep {
        return Ok(());
    }

    // 按修改时间排序，旧的在前面
    backups.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    // 删除多余的旧备份
    for entry in backups.iter().take(backups.len().saturating_sub(keep)) {
        let _ = fs::remove_file(entry.path());
    }

    Ok(())
}
