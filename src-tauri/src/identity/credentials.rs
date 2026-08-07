//! Identity 管理 — Credential 类型注册表 + 敏感值文件存储
//!
//! 核心设计:
//! - DB `identity_credentials.data` 中敏感字段存储 `sha256:<hex>` 指纹
//! - 实际 secret 值存储在 `{app_config_dir}/credentials/{identity_id}/{credential_type}.env`
//! - 文件权限 0600 (Unix) / Windows ACL (best-effort)
//! - DB 备份和导出不泄露 token

use crate::config::{atomic_write, get_app_config_dir};
use crate::error::AppError;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Credential 字段定义（从 credential_types.schema JSON 解析）
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub key: String,
    pub secret: bool,
    pub optional: bool,
    pub env_key: Option<String>,
}

/// 解析后的 credential type schema
#[derive(Debug, Clone)]
pub struct CredentialTypeSchema {
    pub id: String,
    pub name: String,
    pub fields: Vec<FieldDef>,
}

/// 解析完成的 credential（包含从文件系统加载的 secret 值）
#[derive(Debug, Clone)]
pub struct ResolvedCredential {
    pub credential_type: String,
    /// field_key → field_value (包括 secret 字段的明文值)
    pub fields: HashMap<String, String>,
}

impl CredentialTypeSchema {
    /// 从 credential_types 表的 schema JSON 解析
    pub fn from_db_row(id: &str, name: &str, schema_json: &str) -> Result<Self, AppError> {
        let schema: serde_json::Value = serde_json::from_str(schema_json)
            .map_err(|e| AppError::Config(format!("解析 credential schema 失败: {e}")))?;

        let fields_obj = schema["fields"]
            .as_object()
            .ok_or_else(|| AppError::Config("credential schema 缺少 fields".to_string()))?;

        let mut fields = Vec::new();
        for (key, val) in fields_obj {
            fields.push(FieldDef {
                key: key.clone(),
                secret: val["secret"].as_bool().unwrap_or(false),
                optional: val["optional"].as_bool().unwrap_or(false),
                env_key: val["env_key"].as_str().map(str::to_string),
            });
        }

        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            fields,
        })
    }

    /// 获取所有 secret 字段的 key
    pub fn secret_field_keys(&self) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| f.secret)
            .map(|f| f.key.as_str())
            .collect()
    }

    /// 获取所有必填字段的 key
    pub fn required_field_keys(&self) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| !f.optional)
            .map(|f| f.key.as_str())
            .collect()
    }

    /// 获取字段到 env_key 的映射（非 secret 字段 + env_key 存在的）
    pub fn env_key_map(&self) -> HashMap<String, String> {
        self.fields
            .iter()
            .filter_map(|f| f.env_key.as_ref().map(|ek| (f.key.clone(), ek.clone())))
            .collect()
    }
}

// ============================================================================
// Secret File Storage
// ============================================================================

/// 获取敏感值文件的存储路径
fn secret_file_path(identity_id: &str, credential_type: &str) -> PathBuf {
    get_app_config_dir()
        .join("credentials")
        .join(identity_id)
        .join(format!("{credential_type}.env"))
}

/// 计算文本的 sha256 十六进制指纹
pub fn sha256_fingerprint(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// 检查字符串是否是 sha256 指纹格式
pub fn is_fingerprint(value: &str) -> bool {
    value.starts_with("sha256:")
}

/// 将敏感字段写入文件系统（0600 权限）
///
/// 格式: KEY=VALUE（每行一个）
pub fn write_secret_file(
    identity_id: &str,
    credential_type: &str,
    secrets: &HashMap<String, String>,
) -> Result<(), AppError> {
    let path = secret_file_path(identity_id, credential_type);

    // 确保目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::io(parent, e))?;
    }

    // 构建 KEY=VALUE 内容
    let mut content = String::new();
    // 写入 managed block header
    content.push_str(&format!(
        "# idswitch: {identity_id} ({credential_type}) -- managed, do not edit\n"
    ));
    for (key, value) in secrets {
        content.push_str(&format!("{key}={value}\n"));
    }
    content.push_str("# /idswitch\n");

    // 原子写入
    atomic_write(&path, content.as_bytes())?;

    // 设置文件权限 0600
    set_restrictive_permissions(&path)?;

    log::info!("Secret file written: {}", path.display());
    Ok(())
}

/// 从文件系统读取敏感值
///
/// 返回 key → value 的 HashMap
pub fn read_secret_file(
    identity_id: &str,
    credential_type: &str,
) -> Result<HashMap<String, String>, AppError> {
    let path = secret_file_path(identity_id, credential_type);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| AppError::io(&path, e))?;

    let mut result = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // 跳过注释和空行
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // 解析 KEY=VALUE
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();
            if !key.is_empty() {
                result.insert(key, value);
            }
        }
    }

    Ok(result)
}

/// 删除敏感值文件
pub fn delete_secret_file(identity_id: &str, credential_type: &str) -> Result<(), AppError> {
    let path = secret_file_path(identity_id, credential_type);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| AppError::io(&path, e))?;
        log::info!("Secret file deleted: {}", path.display());
    }
    // 尝试清理空的 identity 目录
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let _ = fs::remove_dir(parent); // 忽略非空目录错误
        }
    }
    Ok(())
}

/// 设置文件权限为仅 owner 可读写
#[cfg(unix)]
fn set_restrictive_permissions(path: &std::path::Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| AppError::io(path, e))?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms).map_err(|e| AppError::io(path, e))?;
    Ok(())
}

/// Windows: best-effort 限制文件权限（用户目录已有 ACL 保护）
#[cfg(not(unix))]
fn set_restrictive_permissions(_path: &std::path::Path) -> Result<(), AppError> {
    // Windows 用户 profile 目录已有 ACL 保护
    // 未来可添加 icacls 调用加强限制
    Ok(())
}

// ============================================================================
// Credential Resolution
// ============================================================================

/// 构建用于 DB 存储的 data JSON
///
/// - 非 secret 字段: 存原值
/// - secret 字段: 存 sha256 指纹
pub fn build_credential_data_json(
    schema: &CredentialTypeSchema,
    fields: &HashMap<String, String>,
) -> Result<String, AppError> {
    let mut data = serde_json::Map::new();

    for field_def in &schema.fields {
        let value = fields.get(&field_def.key).cloned().unwrap_or_default();

        if field_def.secret && !value.is_empty() {
            // Secret 字段: 存 sha256 指纹
            data.insert(
                field_def.key.clone(),
                serde_json::Value::String(sha256_fingerprint(&value)),
            );
        } else {
            // 非 secret 字段: 存原值
            data.insert(
                field_def.key.clone(),
                serde_json::Value::String(value),
            );
        }
    }

    serde_json::to_string(&data)
        .map_err(|e| AppError::Config(format!("序列化 credential data 失败: {e}")))
}

/// 验证输入的 fields 是否满足 schema 要求
pub fn validate_credential_fields(
    schema: &CredentialTypeSchema,
    fields: &HashMap<String, String>,
) -> Result<(), AppError> {
    let required = schema.required_field_keys();

    // 检查非 secret 必填字段
    for key in &required {
        let is_secret = schema
            .fields
            .iter()
            .any(|f| f.key == *key && f.secret);
        if !is_secret {
            let value = fields.get(*key).map(|s| s.as_str()).unwrap_or("");
            if value.trim().is_empty() {
                return Err(AppError::Config(format!(
                    "缺少必填字段: {key}"
                )));
            }
        }
    }

    Ok(())
}

/// 从 DB data JSON + 文件系统解析完整的 credential
///
/// - 非 secret 字段: 直接从 data JSON 取值
/// - secret 字段: 从 data JSON 取指纹 vs 文件系统实际值做校验，返回文件系统的明文值
pub fn resolve_credential(
    identity_id: &str,
    schema: &CredentialTypeSchema,
    db_data_json: &str,
) -> Result<ResolvedCredential, AppError> {
    let db_data: serde_json::Value = serde_json::from_str(db_data_json)
        .map_err(|e| AppError::Config(format!("解析 credential data 失败: {e}")))?;

    // 读取文件系统 secret 值
    let secrets = read_secret_file(identity_id, &schema.id)?;

    let mut fields = HashMap::new();

    for field_def in &schema.fields {
        let db_value = db_data
            .get(&field_def.key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if field_def.secret {
            // Secret 字段: 从文件系统获取
            // 查找匹配的 env_key
            let env_key = field_def
                .env_key
                .as_deref()
                .unwrap_or(&field_def.key);

            if let Some(secret_value) = secrets.get(env_key) {
                if !secret_value.is_empty() {
                    // 验证指纹一致性
                    let expected_fingerprint = db_value.clone();
                    let actual_fingerprint = sha256_fingerprint(secret_value);

                    if is_fingerprint(&expected_fingerprint)
                        && expected_fingerprint != actual_fingerprint
                    {
                        log::warn!(
                            "Identity '{}' credential '{}': fingerprint mismatch for field '{}'",
                            identity_id,
                            schema.id,
                            field_def.key
                        );
                    }

                    fields.insert(field_def.key.clone(), secret_value.clone());
                } else if field_def.optional {
                    fields.insert(field_def.key.clone(), String::new());
                } else {
                    return Err(AppError::Config(format!(
                        "Identity '{}' 缺少必需 secret 字段 '{}'，请重新添加 credential",
                        identity_id, field_def.key
                    )));
                }
            } else if field_def.optional {
                fields.insert(field_def.key.clone(), String::new());
            } else {
                return Err(AppError::Config(format!(
                    "Identity '{}' 的 secret 文件缺失字段 '{}'，请重新添加 credential: {}",
                    identity_id,
                    field_def.key,
                    secret_file_path(identity_id, &schema.id).display()
                )));
            }
        } else {
            // 非 secret 字段: 直接从 DB 取值
            fields.insert(field_def.key.clone(), db_value);
        }
    }

    Ok(ResolvedCredential {
        credential_type: schema.id.clone(),
        fields,
    })
}
