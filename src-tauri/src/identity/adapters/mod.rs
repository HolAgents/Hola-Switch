//! Identity 管理 — Target Adapter 抽象层
//!
//! 每个 Target 对应一个 Adapter 实现，负责:
//! - 将 Credential 的字段注入到目标配置文件中
//! - 从目标配置文件移除 identity 注入的 key
//! - 检测当前 live config 中的绑定状态
//!
//! 扩展新 Target 只需实现 TargetAdapter trait + 注册到 registry。

use crate::error::AppError;
use crate::identity::credentials::ResolvedCredential;
use crate::identity::dao::IdentityTarget;
use std::collections::HashMap;

pub mod claude_code;
pub mod hermes;

/// 绑定状态的实时检测结果
#[derive(Debug, Clone)]
pub struct BoundState {
    /// 当前绑定的 identity 名称（从 live config 中读取）
    pub identity_name: Option<String>,
    /// live config 中存在的 managed key 列表
    pub keys_present: Vec<String>,
    /// 是否完整（所有 managed key 都存在）
    pub complete: bool,
}

/// Target 适配器 trait
///
/// 所有方法都是同步的（读写本地文件，无网络操作）。
pub trait TargetAdapter: Send + Sync {
    /// 适配器标识符，对应 identity_targets.adapter
    fn id(&self) -> &'static str;

    /// 此适配器接受的 credential 类型列表
    fn accepted_types(&self) -> &'static [&'static str];

    /// 将 credentials 注入目标配置文件
    ///
    /// 采用 surgical merge 模式：只修改 adapter 管理的 key，保留文件中所有其他内容。
    /// 写入前创建备份。
    fn bind(
        &self,
        target: &IdentityTarget,
        identity_name: &str,
        creds: &HashMap<String, ResolvedCredential>,
    ) -> Result<(), AppError>;

    /// 从目标配置文件移除 identity 注入的 key
    fn unbind(&self, target: &IdentityTarget) -> Result<(), AppError>;

    /// 检查当前 live config 中的绑定状态
    fn is_bound(&self, target: &IdentityTarget) -> Result<BoundState, AppError>;
}

/// Adapter 注册表
pub struct AdapterRegistry {
    // Phase 1: 简单静态注册。后续可改为 HashMap 动态注册。
}

impl AdapterRegistry {
    /// 根据 adapter ID 获取对应的 adapter 实例
    pub fn get(adapter_id: &str) -> Result<Box<dyn TargetAdapter>, AppError> {
        match adapter_id {
            "claude-code" => Ok(Box::new(claude_code::ClaudeCodeAdapter)),
            "hermes" => Ok(Box::new(hermes::HermesAdapter)),
            _ => Err(AppError::Config(format!(
                "未知的 adapter 类型: {adapter_id}"
            ))),
        }
    }
}
