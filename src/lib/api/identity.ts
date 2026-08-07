import { invoke } from "@tauri-apps/api/core";

export interface CredentialInfo {
  credential_type: string;
  /** 非敏感字段原值, 敏感字段显示为 "sha256:..." */
  fields: Record<string, string>;
  created_at: string;
}

export interface IdentityInfo {
  id: string;
  name: string;
  description: string;
  created_at: string;
  credentials: CredentialInfo[];
}

export interface StatusRow {
  target_id: string;
  target_label: string;
  identity_name: string | null;
  active_credentials: string[];
  live_status: string;
}

export interface AuditRow {
  timestamp: string;
  action: string;
  identity_name: string | null;
  target_id: string | null;
  details: string;
  result: string;
}

export interface AuditFilter {
  identity?: string;
  target?: string;
  limit?: number;
}

export interface CredentialAddResult {
  ssh_key_path: string;
  ssh_key_uploaded: boolean;
  ssh_public_key: string | null;
  ssh_setup_url: string;
}

export const identityApi = {
  async create(name: string, description?: string): Promise<IdentityInfo> {
    return await invoke("identity_create", {
      name,
      description: description ?? null,
    });
  },

  async list(): Promise<IdentityInfo[]> {
    return await invoke("identity_list");
  },

  async remove(name: string): Promise<void> {
    return await invoke("identity_delete", { name });
  },

  async addGithubCredential(input: {
    identity: string;
    token: string;
    gitName: string;
    gitEmail: string;
    sshKeyPath?: string;
  }): Promise<CredentialAddResult> {
    return await invoke("credential_add_github", {
      identity: input.identity,
      token: input.token,
      gitName: input.gitName,
      gitEmail: input.gitEmail,
      sshKeyPath: input.sshKeyPath ?? null,
    });
  },

  async removeCredential(
    identity: string,
    credentialType: string,
  ): Promise<void> {
    return await invoke("credential_remove", { identity, credentialType });
  },

  async bind(identity: string, target: string): Promise<void> {
    return await invoke("identity_bind", { identity, target });
  },

  async unbind(target: string): Promise<void> {
    return await invoke("identity_unbind", { target });
  },

  async status(): Promise<StatusRow[]> {
    return await invoke("identity_status");
  },

  async audit(filter?: AuditFilter): Promise<AuditRow[]> {
    return await invoke("identity_audit", {
      identity: filter?.identity ?? null,
      target: filter?.target ?? null,
      limit: filter?.limit ?? null,
    });
  },
};
