//! Identity 管理 — Hermes Adapter
//!
//! 管理 `{hermes_dir}/.env` 中的 identity env vars（Hermes 主配置文件）。
//!
//! 使用 managed block 格式:
//! ```
//! # idswitch: holagent001 (github-account) -- managed, do not edit
//! GITHUB_TOKEN=ghp_xxx
//! GIT_AUTHOR_NAME=holagent001
//! GIT_COMMITTER_NAME=holagent001
//! GIT_AUTHOR_EMAIL=hola001@agent.qq.com
//! GIT_COMMITTER_EMAIL=hola001@agent.qq.com
//! # /idswitch
//! ```
//!
//! 行为:
//! - bind(): 读取 .env → upsert managed block → 保留文件其他内容 → atomic_write
//! - unbind(): 删除 managed block → 保留文件其他内容
//! - is_bound(): 检查 managed block 是否存在及完整

use crate::config::atomic_write;
use crate::error::AppError;
use crate::identity::adapters::{BoundState, TargetAdapter};
use crate::identity::credentials::ResolvedCredential;
use crate::identity::dao::IdentityTarget;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Managed block 的开始标记
const BLOCK_HEADER_PREFIX: &str = "# idswitch:";
const BLOCK_FOOTER: &str = "# /idswitch";

pub struct HermesAdapter;

impl HermesAdapter {
    /// 获取 Hermes 主 .env 文件路径
    ///
    /// 如果 target config 中有 `hermes_home` 字段，使用该目录；
    /// 否则回退到默认 `get_hermes_dir()`。
    /// 通过不同 target 指向不同 `hermes_home` 实现 profile 级 identity 分离。
    fn hermes_env_path(target: &IdentityTarget) -> Result<PathBuf, AppError> {
        let config: serde_json::Value = serde_json::from_str(&target.config)
            .map_err(|e| AppError::Config(format!("解析 target config 失败: {e}")))?;

        let hermes_dir = if let Some(home) = config["hermes_home"].as_str() {
            // 展开 ~ 为用户主目录
            if home.starts_with("~/") {
                let home_dir = crate::config::get_home_dir();
                home_dir.join(&home[2..])
            } else if home == "~" {
                crate::config::get_home_dir()
            } else {
                PathBuf::from(home)
            }
        } else {
            crate::hermes_config::get_hermes_dir()
        };

        Ok(hermes_dir.join(".env"))
    }
}

impl TargetAdapter for HermesAdapter {
    fn id(&self) -> &'static str {
        "hermes"
    }

    fn accepted_types(&self) -> &'static [&'static str] {
        &["github-account"]
    }

    fn bind(
        &self,
        target: &IdentityTarget,
        identity_name: &str,
        creds: &HashMap<String, ResolvedCredential>,
    ) -> Result<(), AppError> {
        let path = Self::hermes_env_path(target)?;

        // 读取现有内容
        let content = if path.exists() {
            fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?
        } else {
            String::new()
        };

        // 从 github-account credential 提取值
        let gh_cred = creds.get("github-account").ok_or_else(|| {
            AppError::Config("缺少 github-account credential".to_string())
        })?;

        let token = gh_cred.fields.get("token").map(|s| s.as_str()).unwrap_or("");
        let git_name = gh_cred.fields.get("git_name").map(|s| s.as_str()).unwrap_or("");
        let git_email = gh_cred.fields.get("git_email").map(|s| s.as_str()).unwrap_or("");
        let ssh_key_path = gh_cred.fields.get("ssh_key_path").map(|s| s.as_str()).unwrap_or("");

        // 构建 managed block 的 env var 行
        let mut env_lines = String::new();
        if !token.is_empty() {
            // GH_TOKEN / GITHUB_TOKEN: gh CLI API 操作使用
            env_lines.push_str(&format!("GITHUB_TOKEN={token}\n"));
            env_lines.push_str(&format!("GH_TOKEN={token}\n"));
        }
        if !git_name.is_empty() {
            env_lines.push_str(&format!("GIT_AUTHOR_NAME={git_name}\n"));
            env_lines.push_str(&format!("GIT_COMMITTER_NAME={git_name}\n"));
        }
        if !git_email.is_empty() {
            env_lines.push_str(&format!("GIT_AUTHOR_EMAIL={git_email}\n"));
            env_lines.push_str(&format!("GIT_COMMITTER_EMAIL={git_email}\n"));
        }
        // SSH: 绕过 HTTPS credential helper，push 直接走 SSH key 认证
        if !ssh_key_path.is_empty() {
            env_lines.push_str(&format!("GIT_SSH_COMMAND=ssh -i {ssh_key_path}\n"));
        }

        // 构建新的 managed block
        let new_block = format!(
            "{} {identity_name} (github-account) -- managed, do not edit\n{env_lines}{BLOCK_FOOTER}\n",
            BLOCK_HEADER_PREFIX,
        );

        // 行级处理：移除所有已有的 managed block，然后追加新的
        let mut lines: Vec<&str> = content.lines().collect();
        // 删除所有 managed block 行（# idswitch: 到 # /idswitch 之间的所有行）
        let mut i = 0;
        while i < lines.len() {
            if lines[i].contains(BLOCK_HEADER_PREFIX) {
                // 找到 header，删除直到 footer（含）
                while i < lines.len() {
                    let is_footer = lines[i].trim() == BLOCK_FOOTER;
                    lines.remove(i);
                    if is_footer {
                        break;
                    }
                }
            } else {
                i += 1;
            }
        }
        // 重建内容并追加新 block
        let preserved = lines.join("\n");
        let new_content = if preserved.is_empty() {
            new_block
        } else {
            format!("{preserved}\n{new_block}")
        };

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }

        // 原子写入
        atomic_write(&path, new_content.as_bytes())?;

        log::info!(
            "HermesAdapter: bound identity '{}' to {}",
            identity_name,
            path.display()
        );
        Ok(())
    }

    fn unbind(&self, target: &IdentityTarget) -> Result<(), AppError> {
        let path = Self::hermes_env_path(target)?;

        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;

        // 行级处理：移除所有 managed block 行
        let mut lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if lines[i].contains(BLOCK_HEADER_PREFIX) {
                while i < lines.len() {
                    let is_footer = lines[i].trim() == BLOCK_FOOTER;
                    lines.remove(i);
                    if is_footer {
                        break;
                    }
                }
            } else {
                i += 1;
            }
        }
        // 清理尾部空行
        while lines.last().map_or(false, |l| l.trim().is_empty()) {
            lines.pop();
        }
        let new_content = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };

        atomic_write(&path, new_content.as_bytes())?;

        log::info!("HermesAdapter: unbound from {}", path.display());
        Ok(())
    }

    fn is_bound(&self, target: &IdentityTarget) -> Result<BoundState, AppError> {
        let path = Self::hermes_env_path(target)?;

        if !path.exists() {
            return Ok(BoundState {
                identity_name: None,
                keys_present: Vec::new(),
                complete: false,
            });
        }

        let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;

        let (identity_name, keys) = match find_managed_block(&content) {
            Some((header, block_content)) => {
                // 解析 header: "# idswitch: name (type) -- managed, do not edit"
                let name = header
                    .strip_prefix(BLOCK_HEADER_PREFIX)
                    .and_then(|rest| rest.split(" (").next())
                    .map(|s| s.trim().to_string());

                let mut keys_present = Vec::new();
                for line in block_content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        if let Some(eq_pos) = trimmed.find('=') {
                            keys_present.push(trimmed[..eq_pos].trim().to_string());
                        }
                    }
                }

                (name, keys_present)
            }
            None => (None, Vec::new()),
        };

        let complete = !keys.is_empty();

        Ok(BoundState {
            identity_name,
            keys_present: keys,
            complete,
        })
    }
}

// ============================================================================
// Managed Block 解析辅助函数
// ============================================================================

/// 查找 managed block 的内容（不含 header/footer 行）
///
/// 返回 (header_line, block_body)
fn find_managed_block(content: &str) -> Option<(String, String)> {
    let mut in_block = false;
    let mut header = None;
    let mut block_start = 0;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(BLOCK_HEADER_PREFIX) {
            in_block = true;
            header = Some(trimmed.to_string());
            block_start = i + 1;
        } else if in_block && trimmed == BLOCK_FOOTER {
            // 提取 block 内容（header 下一行到 footer 前一行）
            let lines: Vec<&str> = content.lines().collect();
            let body = lines[block_start..i].join("\n");
            return Some((header?, body));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_managed_block() {
        let content = "EXISTING_KEY=value\n# idswitch: test-id (github-account) -- managed, do not edit\nGITHUB_TOKEN=ghp_test\n# /idswitch\nOTHER_KEY=other\n";
        let (header, body) = find_managed_block(content).expect("should find block");
        assert!(header.contains("test-id"));
        assert!(body.contains("GITHUB_TOKEN=ghp_test"));
    }

    #[test]
    fn test_find_managed_block_not_found() {
        let content = "EXISTING_KEY=value\nOTHER_KEY=other\n";
        assert!(find_managed_block(content).is_none());
    }
}
