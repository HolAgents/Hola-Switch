//! Identity 管理 — 单元测试与集成测试
//!
//! 测试覆盖:
//! - Credential 存储与解析 (sha256 指纹 + 文件系统)
//! - Adapter bind/unbind (Claude Code + Hermes)
//! - Binder 引擎
//! - 审计日志

#[cfg(test)]
mod tests {
    use crate::database::Database;
    use crate::error::AppError;
    use crate::identity::adapters::TargetAdapter;
    use crate::identity::credentials::{
        build_credential_data_json, delete_secret_file, is_fingerprint, read_secret_file,
        resolve_credential, sha256_fingerprint, validate_credential_fields, write_secret_file,
        CredentialTypeSchema, ResolvedCredential,
    };
    use crate::identity::dao::IdentityTarget;
    use serde_json::json;
    use std::collections::HashMap;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // ========================================================================
    // Helpers
    // ========================================================================

    fn test_db() -> Database {
        Database::memory().expect("memory db")
    }

    fn github_schema() -> CredentialTypeSchema {
        CredentialTypeSchema::from_db_row(
            "github-account",
            "GitHub Account",
            r#"{"fields":{"token":{"secret":true,"env_key":"GITHUB_TOKEN"},"git_name":{"secret":false,"env_key":"GIT_AUTHOR_NAME"},"git_email":{"secret":false,"env_key":"GIT_AUTHOR_EMAIL"},"ssh_key_path":{"secret":false,"optional":true}}}"#,
        )
        .expect("parse schema")
    }

    fn create_test_identity(db: &Database, name: &str) -> Result<String, AppError> {
        // Run identity migration first (memory DB doesn't auto-migrate)
        let conn = crate::database::lock_conn!(db.conn);
        crate::identity::schema::migrate_v16_to_v17(&conn)?;
        drop(conn);

        // Manually insert since identity_create needs full DB setup
        let id = uuid::Uuid::new_v4().to_string();
        db.identity_insert(&id, name, "")?;
        // Seed credential_types
        let conn = crate::database::lock_conn!(db.conn);
        conn.execute(
            "INSERT OR IGNORE INTO credential_types (id, name, schema) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "github-account",
                "GitHub Account",
                r#"{"fields":{"token":{"secret":true,"env_key":"GITHUB_TOKEN"},"git_name":{"secret":false,"env_key":"GIT_AUTHOR_NAME"},"git_email":{"secret":false,"env_key":"GIT_AUTHOR_EMAIL"},"ssh_key_path":{"secret":false,"optional":true}}}"#
            ],
        )
        .map_err(|e| AppError::Database(format!("seed credential_type: {e}")))?;
        // Seed identity_targets
        conn.execute(
            "INSERT OR IGNORE INTO identity_targets (id, adapter, label, config) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["claude-code", "claude-code", "Claude Code CLI", "{}"],
        )
        .map_err(|e| AppError::Database(format!("seed target: {e}")))?;
        conn.execute(
            "INSERT OR IGNORE INTO identity_targets (id, adapter, label, config) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["hermes:ops", "hermes", "Hermes (ops)", r#"{"profile":"ops"}"#],
        )
        .map_err(|e| AppError::Database(format!("seed target hermes: {e}")))?;
        Ok(id)
    }

    // ========================================================================
    // Credential Schema Tests
    // ========================================================================

    #[test]
    fn test_parse_credential_schema() {
        let schema = github_schema();
        assert_eq!(schema.id, "github-account");

        let token_field = schema.fields.iter().find(|f| f.key == "token").unwrap();
        assert!(token_field.secret);
        assert_eq!(token_field.env_key.as_deref(), Some("GITHUB_TOKEN"));

        let name_field = schema.fields.iter().find(|f| f.key == "git_name").unwrap();
        assert!(!name_field.secret);
        assert!(!name_field.optional);
    }

    #[test]
    fn test_secret_field_keys() {
        let schema = github_schema();
        let secrets = schema.secret_field_keys();
        assert_eq!(secrets, vec!["token"]);
    }

    #[test]
    fn test_required_field_keys() {
        let schema = github_schema();
        let required = schema.required_field_keys();
        assert!(required.contains(&"token"));
        assert!(required.contains(&"git_name"));
        assert!(required.contains(&"git_email"));
        assert!(!required.contains(&"ssh_key_path")); // optional
    }

    // ========================================================================
    // SHA256 Fingerprint Tests
    // ========================================================================

    #[test]
    fn test_sha256_fingerprint() {
        let fp = sha256_fingerprint("test-token");
        assert!(fp.starts_with("sha256:"));
        assert_eq!(fp.len(), 71); // "sha256:" + 64 hex chars
    }

    #[test]
    fn test_is_fingerprint() {
        assert!(is_fingerprint("sha256:abc123"));
        assert!(!is_fingerprint("plain-text"));
        assert!(!is_fingerprint(""));
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let fp1 = sha256_fingerprint("hello");
        let fp2 = sha256_fingerprint("hello");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprint_different_inputs() {
        let fp1 = sha256_fingerprint("token-a");
        let fp2 = sha256_fingerprint("token-b");
        assert_ne!(fp1, fp2);
    }

    // ========================================================================
    // Credential Data JSON Tests
    // ========================================================================

    #[test]
    fn test_build_credential_data_json() {
        let schema = github_schema();
        let mut fields = HashMap::new();
        fields.insert("token".to_string(), "ghp_secret".to_string());
        fields.insert("git_name".to_string(), "test-user".to_string());
        fields.insert("git_email".to_string(), "test@example.com".to_string());
        fields.insert("ssh_key_path".to_string(), "".to_string());

        let json = build_credential_data_json(&schema, &fields).expect("build json");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");

        // token should be a fingerprint
        let token_val = parsed["token"].as_str().unwrap();
        assert!(is_fingerprint(token_val));

        // git_name should be plain text
        assert_eq!(parsed["git_name"].as_str().unwrap(), "test-user");
        assert_eq!(parsed["git_email"].as_str().unwrap(), "test@example.com");
    }

    #[test]
    fn test_validate_credential_fields_missing_required() {
        let schema = github_schema();
        let mut fields = HashMap::new();
        fields.insert("token".to_string(), "ghp_secret".to_string());
        // missing git_name and git_email
        let result = validate_credential_fields(&schema, &fields);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_credential_fields_ok() {
        let schema = github_schema();
        let mut fields = HashMap::new();
        fields.insert("token".to_string(), "ghp_secret".to_string());
        fields.insert("git_name".to_string(), "test-user".to_string());
        fields.insert("git_email".to_string(), "test@example.com".to_string());
        fields.insert("ssh_key_path".to_string(), "".to_string());
        assert!(validate_credential_fields(&schema, &fields).is_ok());
    }

    // ========================================================================
    // Secret File Tests
    // ========================================================================

    #[test]
    #[serial]
    fn test_write_and_read_secret_file() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path().to_string_lossy().to_string());

        let mut secrets = HashMap::new();
        secrets.insert("GITHUB_TOKEN".to_string(), "ghp_test123".to_string());

        write_secret_file("test-id", "github-account", &secrets).expect("write secret file");

        let read = read_secret_file("test-id", "github-account").expect("read secret file");
        assert_eq!(read.get("GITHUB_TOKEN").map(|s| s.as_str()), Some("ghp_test123"));
    }

    #[test]
    #[serial]
    fn test_delete_secret_file() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path().to_string_lossy().to_string());

        let mut secrets = HashMap::new();
        secrets.insert("GITHUB_TOKEN".to_string(), "ghp_test123".to_string());
        write_secret_file("test-del", "github-account", &secrets).expect("write");

        delete_secret_file("test-del", "github-account").expect("delete");
        let read = read_secret_file("test-del", "github-account").expect("read after delete");
        assert!(read.is_empty());
    }

    // ========================================================================
    // Credential Resolution Tests
    // ========================================================================

    #[test]
    #[serial]
    fn test_resolve_credential_roundtrip() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path().to_string_lossy().to_string());

        let schema = github_schema();

        // 1. Build data JSON (secret → fingerprint)
        let mut fields = HashMap::new();
        fields.insert("token".to_string(), "ghp_real_token".to_string());
        fields.insert("git_name".to_string(), "test-user".to_string());
        fields.insert("git_email".to_string(), "test@example.com".to_string());
        fields.insert("ssh_key_path".to_string(), "/home/user/.ssh/id_rsa".to_string());

        let data_json = build_credential_data_json(&schema, &fields).expect("build data");

        // 2. Write secret file
        let mut secrets = HashMap::new();
        secrets.insert("GITHUB_TOKEN".to_string(), "ghp_real_token".to_string());
        write_secret_file("test-roundtrip", "github-account", &secrets).expect("write secret");

        // 3. Resolve credential
        let resolved = resolve_credential("test-roundtrip", &schema, &data_json)
            .expect("resolve credential");

        assert_eq!(resolved.credential_type, "github-account");
        assert_eq!(
            resolved.fields.get("token").map(|s| s.as_str()),
            Some("ghp_real_token")
        );
        assert_eq!(
            resolved.fields.get("git_name").map(|s| s.as_str()),
            Some("test-user")
        );
        assert_eq!(
            resolved.fields.get("git_email").map(|s| s.as_str()),
            Some("test@example.com")
        );
        assert_eq!(
            resolved.fields.get("ssh_key_path").map(|s| s.as_str()),
            Some("/home/user/.ssh/id_rsa")
        );
    }

    // ========================================================================
    // DAO Tests
    // ========================================================================

    #[test]
    fn test_identity_crud() {
        let db = test_db();
        let _id = create_test_identity(&db, "test-identity");

        // Read
        let identity = db.identity_get_by_name("test-identity").expect("get");
        assert!(identity.is_some());
        assert_eq!(identity.as_ref().unwrap().name, "test-identity");

        // List
        let list = db.identity_list().expect("list");
        assert_eq!(list.len(), 1);

        // Delete
        db.identity_delete("test-identity").expect("delete");
        let after = db.identity_get_by_name("test-identity").expect("get after delete");
        assert!(after.is_none());
    }

    #[test]
    fn test_identity_credential_insert_and_get() {
        let db = test_db();
        let id = create_test_identity(&db, "test-cred").expect("create identity");

        let data = json!({"token": "sha256:abc", "git_name": "user", "git_email": "u@q.com"}).to_string();
        db.identity_credential_insert(&id, "github-account", &data).expect("insert cred");

        let cred = db.identity_credential_get(&id, "github-account").expect("get cred");
        assert!(cred.is_some());
        assert_eq!(cred.unwrap().credential_type, "github-account");
    }

    #[test]
    fn test_identity_binding_upsert_and_get() {
        let db = test_db();
        let id = create_test_identity(&db, "test-bind").expect("create identity");

        db.identity_binding_upsert(&id, "claude-code").expect("upsert bind");

        let binding = db.identity_binding_get_by_target("claude-code").expect("get bind");
        assert!(binding.is_some());
        assert_eq!(binding.unwrap().identity_id, id);
    }

    #[test]
    fn test_identity_binding_unique_target() {
        let db = test_db();
        let id1 = create_test_identity(&db, "test-bind-1").expect("create id1");

        // create second identity manually
        let id2 = uuid::Uuid::new_v4().to_string();
        db.identity_insert(&id2, "test-bind-2", "").expect("insert id2");

        db.identity_binding_upsert(&id1, "claude-code").expect("first bind");
        // Second bind to same target should replace
        db.identity_binding_upsert(&id2, "claude-code").expect("second bind");

        let binding = db.identity_binding_get_by_target("claude-code").expect("get bind");
        assert_eq!(binding.unwrap().identity_id, id2); // replaced
    }

    // ========================================================================
    // Audit Tests
    // ========================================================================

    #[test]
    fn test_audit_log() {
        let db = test_db();
        let id = create_test_identity(&db, "test-audit").expect("create identity");

        db.identity_audit_log("identity.create", Some(&id), None, r#"{"name":"test-audit"}"#)
            .expect("audit create");
        db.identity_audit_log("bind", Some(&id), Some("claude-code"), "{}")
            .expect("audit bind");

        let entries = db.identity_audit_query(None, None, Some(10)).expect("audit query");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "bind"); // most recent first
        assert_eq!(entries[1].action, "identity.create");
    }

    // ========================================================================
    // ClaudeCodeAdapter Tests
    // ========================================================================

    #[test]
    #[serial]
    fn test_claude_code_adapter_bind_unbind() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path().to_string_lossy().to_string());

        // Create a fake settings.json
        let claude_dir = temp.path().join(".claude");
        fs::create_dir_all(&claude_dir).expect("create .claude dir");
        let settings_path = claude_dir.join("settings.json");

        let initial = json!({
            "model": "claude-sonnet-4-5",
            "permissions": {"allow": ["Read", "Write"]},
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        });
        fs::write(&settings_path, serde_json::to_string_pretty(&initial).unwrap())
            .expect("write initial settings");

        let adapter = crate::identity::adapters::claude_code::ClaudeCodeAdapter;
        let target = IdentityTarget {
            id: "claude-code".to_string(),
            adapter: "claude-code".to_string(),
            label: "Claude Code".to_string(),
            config: "{}".to_string(),
            enabled: true,
        };

        // Build a resolved credential
        let mut cred_fields = HashMap::new();
        cred_fields.insert("token".to_string(), "ghp_claude_test".to_string());
        cred_fields.insert("git_name".to_string(), "claude-user".to_string());
        cred_fields.insert("git_email".to_string(), "claude@example.com".to_string());

        let resolved = ResolvedCredential {
            credential_type: "github-account".to_string(),
            fields: cred_fields,
        };
        let mut creds_map = HashMap::new();
        creds_map.insert("github-account".to_string(), resolved);

        // Bind
        adapter.bind(&target, "test-identity", &creds_map).expect("bind");

        // Verify settings.json
        let settings: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&settings_path).unwrap(),
        )
        .unwrap();

        // Identity env vars should be present
        assert_eq!(settings["env"]["GITHUB_TOKEN"], json!("ghp_claude_test"));
        assert_eq!(settings["env"]["GIT_AUTHOR_NAME"], json!("claude-user"));
        assert_eq!(settings["env"]["GIT_AUTHOR_EMAIL"], json!("claude@example.com"));

        // Original keys should be preserved
        assert_eq!(settings["env"]["ANTHROPIC_BASE_URL"], json!("https://api.anthropic.com"));
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-ant-test"));
        assert_eq!(settings["model"], json!("claude-sonnet-4-5"));
        assert_eq!(settings["permissions"]["allow"][0], json!("Read"));

        // Unbind
        adapter.unbind(&target).expect("unbind");

        let settings_after: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&settings_path).unwrap(),
        )
        .unwrap();

        // Identity env vars should be removed
        assert!(settings_after["env"].get("GITHUB_TOKEN").is_none());
        assert!(settings_after["env"].get("GIT_AUTHOR_NAME").is_none());

        // Original keys should still be preserved
        assert_eq!(settings_after["env"]["ANTHROPIC_BASE_URL"], json!("https://api.anthropic.com"));
        assert_eq!(settings_after["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-ant-test"));
    }

    #[test]
    #[serial]
    fn test_claude_code_adapter_is_bound() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path().to_string_lossy().to_string());

        let claude_dir = temp.path().join(".claude");
        fs::create_dir_all(&claude_dir).expect("create .claude dir");

        let settings = json!({
            "env": {
                "GITHUB_TOKEN": "ghp_test",
                "GH_TOKEN": "ghp_test",
                "GIT_AUTHOR_NAME": "user",
                "GIT_AUTHOR_EMAIL": "u@q.com",
                "GIT_COMMITTER_NAME": "user",
                "GIT_COMMITTER_EMAIL": "u@q.com",
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
            },
            "_idswitch_identity": "test-identity"
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .expect("write settings");

        let adapter = crate::identity::adapters::claude_code::ClaudeCodeAdapter;
        let target = IdentityTarget {
            id: "claude-code".to_string(),
            adapter: "claude-code".to_string(),
            label: "Claude Code".to_string(),
            config: "{}".to_string(),
            enabled: true,
        };

        let state = adapter.is_bound(&target).expect("is_bound");
        assert!(state.complete);
        assert_eq!(state.identity_name.as_deref(), Some("test-identity"));
        assert!(state.complete, "Expected complete binding with all 6 git identity keys");
        assert_eq!(state.keys_present.len(), 6); // GITHUB_TOKEN, GH_TOKEN, GIT_AUTHOR_NAME/EMAIL, GIT_COMMITTER_NAME/EMAIL
    }

    // ========================================================================
    // HermesAdapter Tests
    // ========================================================================

    #[test]
    #[serial]
    fn test_hermes_adapter_bind_unbind() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path().to_string_lossy().to_string());

        // Use a custom hermes dir via env var
        let hermes_dir = temp.path().join("hermes_test");
        std::env::set_var("HERMES_HOME", hermes_dir.to_string_lossy().to_string());

        // Hermes reads the top-level .env, not profiles/{profile}/.env
        fs::create_dir_all(&hermes_dir).expect("create hermes dir");
        let env_path = hermes_dir.join(".env");

        // Write initial .env content
        fs::write(&env_path, "EXISTING_KEY=existing_value\n# some comment\nOTHER_KEY=other_value\n")
            .expect("write initial .env");

        let adapter = crate::identity::adapters::hermes::HermesAdapter;
        let target = IdentityTarget {
            id: "hermes:default".to_string(),
            adapter: "hermes".to_string(),
            label: "Hermes (default)".to_string(),
            config: r#"{"profile":"default"}"#.to_string(),
            enabled: true,
        };

        let mut cred_fields = HashMap::new();
        cred_fields.insert("token".to_string(), "ghp_hermes_test".to_string());
        cred_fields.insert("git_name".to_string(), "testuser".to_string());
        cred_fields.insert("git_email".to_string(), "test@example.com".to_string());

        let resolved = ResolvedCredential {
            credential_type: "github-account".to_string(),
            fields: cred_fields,
        };
        let mut creds_map = HashMap::new();
        creds_map.insert("github-account".to_string(), resolved);

        // Bind
        adapter.bind(&target, "hermes-identity", &creds_map).expect("bind");

        let content = fs::read_to_string(&env_path).expect("read .env");
        assert!(content.contains("GITHUB_TOKEN=ghp_hermes_test"));
        assert!(content.contains("GIT_AUTHOR_NAME=testuser"));
        assert!(content.contains("GIT_COMMITTER_NAME=testuser"));
        assert!(content.contains("GIT_AUTHOR_EMAIL=test@example.com"));
        assert!(content.contains("GIT_COMMITTER_EMAIL=test@example.com"));
        assert!(content.contains("# idswitch: hermes-identity (github-account)"));
        assert!(content.contains("EXISTING_KEY=existing_value")); // preserved
        assert!(content.contains("OTHER_KEY=other_value")); // preserved

        // Unbind
        adapter.unbind(&target).expect("unbind");

        let content_after = fs::read_to_string(&env_path).expect("read .env after unbind");
        assert!(!content_after.contains("GITHUB_TOKEN=ghp_hermes_test"));
        assert!(!content_after.contains("GIT_AUTHOR_NAME=testuser"));
        assert!(!content_after.contains("# idswitch:"));
        assert!(content_after.contains("EXISTING_KEY=existing_value")); // preserved
        assert!(content_after.contains("OTHER_KEY=other_value")); // preserved
    }
}
