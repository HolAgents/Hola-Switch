import React, { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Copy, Edit3, ExternalLink, Eye, EyeOff, Loader2, Plus, Save, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { FullScreenPanel } from "@/components/common/FullScreenPanel";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { APP_ICON_MAP } from "@/config/appConfig";
import {
  useIdentityList,
  useIdentityStatus,
  useCreateIdentityMutation,
  useAddGithubCredentialMutation,
  useRemoveCredentialMutation,
  useBindIdentityMutation,
  useUnbindIdentityMutation,
} from "@/hooks/useIdentity";

// Inline credential form (no Dialog — avoids portal conflict with FullScreenPanel)
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

function AddGithubInlineForm({
  identityName,
  initial,
  onClose,
}: {
  identityName: string;
  initial?: { gitName?: string; gitEmail?: string; sshKeyPath?: string };
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const addMutation = useAddGithubCredentialMutation();
  const isEdit = !!initial;
  const [token, setToken] = useState("");
  const [gitName, setGitName] = useState(initial?.gitName ?? "");
  const [gitEmail, setGitEmail] = useState(initial?.gitEmail ?? "");
  const [sshKeyPath, setSshKeyPath] = useState(initial?.sshKeyPath ?? "");
  const [showToken, setShowToken] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const canSubmit =
    token.trim() !== "" &&
    gitName.trim() !== "" &&
    EMAIL_PATTERN.test(gitEmail.trim()) &&
    !addMutation.isPending;

  const handleCopyKey = useCallback((key: string) => {
    navigator.clipboard.writeText(key).catch(() => {});
  }, []);

  const handleSubmit = async () => {
    if (!canSubmit) return;
    try {
      const result = await addMutation.mutateAsync({
        identity: identityName,
        token: token.trim(),
        gitName: gitName.trim(),
        gitEmail: gitEmail.trim(),
        sshKeyPath: sshKeyPath.trim() || undefined,
      });
      onClose();
      if (!result.ssh_key_uploaded) {
        setTimeout(() => {
          toast.warning(
            <div className="space-y-2">
              <p className="font-medium">{t("identity.sshUploadFailedToast")}</p>
              <div className="flex items-center gap-1">
                <code className="flex-1 truncate rounded bg-muted px-2 py-1 text-[11px]">
                  {result.ssh_public_key}
                </code>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 shrink-0"
                  onClick={() =>
                    result.ssh_public_key && handleCopyKey(result.ssh_public_key)
                  }
                >
                  <Copy className="h-3.5 w-3.5" />
                </Button>
              </div>
              <a
                href={result.ssh_setup_url}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-xs underline"
              >
                {result.ssh_setup_url}
                <ExternalLink className="h-3 w-3" />
              </a>
            </div>,
            { duration: 12000 },
          );
        }, 200);
      }
    } catch {
      // mutation onError surfaces toast
    }
  };

  return (
    <div className="space-y-3 rounded-lg border border-border-default p-4">
      <h4 className="text-sm font-medium">
        {isEdit ? t("identity.editGithub") : t("identity.addGithub")}
      </h4>
      <div className="space-y-1.5">
        <Label>{t("identity.token")}</Label>
        <div className="relative">
          <Input
            type={showToken ? "text" : "password"}
            value={token}
            autoComplete="off"
            className="pr-9"
            onChange={(e) => setToken(e.target.value)}
          />
          <Button
            variant="ghost"
            size="icon"
            className="absolute right-1 top-1/2 h-7 w-7 -translate-y-1/2"
            onClick={() => setShowToken(!showToken)}
          >
            {showToken ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">{t("identity.maskedHint")}</p>
      </div>
      <div className="space-y-1.5">
        <Label>{t("identity.gitName")}</Label>
        <Input value={gitName} onChange={(e) => setGitName(e.target.value)} />
      </div>
      <div className="space-y-1.5">
        <Label>{t("identity.gitEmail")}</Label>
        <Input
          type="email"
          value={gitEmail}
          onChange={(e) => setGitEmail(e.target.value)}
        />
      </div>
      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
        <CollapsibleTrigger asChild>
          <Button variant="ghost" size="sm" className="flex items-center gap-1 px-0">
            <ChevronDown className={`h-4 w-4 transition-transform ${advancedOpen ? "rotate-0" : "-rotate-90"}`} />
            {t("identity.sshKeyAdvanced")}
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-1.5 pt-1">
          <Label className="text-xs">{t("identity.sshKeyPath")}</Label>
          <Input
            value={sshKeyPath}
            placeholder={t("identity.sshKeyPathPlaceholder")}
            onChange={(e) => setSshKeyPath(e.target.value)}
          />
          <p className="text-xs text-muted-foreground">{t("identity.sshKeyPathHint")}</p>
        </CollapsibleContent>
      </Collapsible>
      <div className="flex justify-end gap-2">
        <Button variant="outline" size="sm" onClick={onClose} disabled={addMutation.isPending}>
          {t("common.cancel")}
        </Button>
        <Button size="sm" onClick={handleSubmit} disabled={!canSubmit}>
          {addMutation.isPending ? (
            <>
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
              {t("identity.tokenValidating")}
            </>
          ) : (
            t("common.confirm")
          )}
        </Button>
      </div>
    </div>
  );
}

function targetAppId(targetId: string): keyof typeof APP_ICON_MAP | null {
  if (targetId === "claude-code") return "claude";
  if (targetId.startsWith("hermes:")) return "hermes";
  return null;
}

function TargetIcon({ targetId }: { targetId: string }) {
  const appId = targetAppId(targetId);
  if (!appId || !(appId in APP_ICON_MAP)) return null;
  const cfg = APP_ICON_MAP[appId as keyof typeof APP_ICON_MAP];
  return <span className="inline-flex">{cfg.icon}</span>;
}

interface IdentityEditPanelProps {
  mode: "create" | "edit";
  identityName?: string;
  onClose: () => void;
}

const IdentityEditPanel: React.FC<IdentityEditPanelProps> = ({
  mode,
  identityName,
  onClose,
}) => {
  const { t } = useTranslation();
  const { data: identities = [] } = useIdentityList();
  const { data: statusRows = [] } = useIdentityStatus();
  const createMutation = useCreateIdentityMutation();
  const removeCredMutation = useRemoveCredentialMutation();
  const bindMutation = useBindIdentityMutation();
  const unbindMutation = useUnbindIdentityMutation();

  const identity = identities.find((id) => id.name === identityName);
  const credTypes = identity?.credentials.map((c) => c.credential_type) ?? [];
  const hasGithub = credTypes.includes("github-account");
  const githubCred = identity?.credentials.find(
    (c) => c.credential_type === "github-account",
  );
  const bound = statusRows.find((r) => r.identity_name === identityName);

  const [internalMode, setInternalMode] = useState(mode);
  const [name, setName] = useState(identity?.name ?? "");
  const [description, setDescription] = useState(identity?.description ?? "");
  const [editingBasic, setEditingBasic] = useState(internalMode === "create");
  const [addGithubOpen, setAddGithubOpen] = useState(false);
  const [editGithubOpen, setEditGithubOpen] = useState(false);
  const [removeCredType, setRemoveCredType] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const title =
    internalMode === "create" ? t("identity.create") : t("identity.editTitle");

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    try {
      if (internalMode === "create") {
        await createMutation.mutateAsync({ name: name.trim(), description: description.trim() });
        // 创建成功后切换为编辑模式，显示 credential/assign 部分
        setInternalMode("edit");
        setEditingBasic(false);
      } else {
        setEditingBasic(false);
      }
    } finally {
      setSaving(false);
    }
  };

  const handleAssign = async (targetId: string) => {
    if (targetId === "__unbind__") {
      if (bound) await unbindMutation.mutateAsync(bound.target_id);
    } else {
      if (!identityName) return;
      await bindMutation.mutateAsync({ identity: identityName, target: targetId });
    }
  };

  const handleRemoveCred = async (credType: string) => {
    if (!identityName) return;
    await removeCredMutation.mutateAsync({
      identity: identityName,
      credentialType: credType,
    });
    setRemoveCredType(null);
  };

  return (
    <>
      <FullScreenPanel
        isOpen
        title={title}
        onClose={onClose}
        footer={undefined}
      >
        <div className="flex flex-col h-full gap-6">
          {/* Basic info */}
          <div className="glass rounded-xl p-6 border border-white/10 space-y-4 flex-shrink-0">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-medium text-foreground">
                {t("identity.basicInfo")}
              </h3>
              {internalMode === "edit" && !editingBasic && (
                <Button variant="ghost" size="sm" className="gap-1" onClick={() => setEditingBasic(true)}>
                  <Edit3 size={14} />
                  {t("common.edit")}
                </Button>
              )}
            </div>
            <div className="space-y-1.5">
              <label className="text-sm font-medium text-foreground">
                {t("identity.name")}
              </label>
              <Input
                value={name}
                disabled={!editingBasic}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("identity.namePlaceholder")}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-sm font-medium text-foreground">
                {t("identity.description")}
              </label>
              <Input
                value={description}
                disabled={!editingBasic}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("identity.descriptionPlaceholder")}
              />
            </div>
            {editingBasic && (
              <div className="flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => {
                  setName(identity?.name ?? "");
                  setDescription(identity?.description ?? "");
                  setEditingBasic(false);
                }}>
                  {t("common.cancel")}
                </Button>
                <Button size="sm" onClick={handleSave} disabled={saving || !name.trim()}>
                  <Save size={14} />
                  {saving ? t("common.saving") : t("common.save")}
                </Button>
              </div>
            )}
          </div>

          {/* Target binding (edit mode only) */}
          {internalMode === "edit" && (
            <div className="glass rounded-xl p-6 border border-white/10 space-y-3 flex-shrink-0">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-foreground">
                  {t("identity.assignTo")}
                </span>
                {bound && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs text-muted-foreground hover:text-destructive"
                    onClick={() => handleAssign("__unbind__")}
                  >
                    {t("identity.unassign")}
                  </Button>
                )}
              </div>
              {bound ? (
                <div className="flex items-center gap-1.5 rounded-lg border border-border-default px-3 py-2">
                  <TargetIcon targetId={bound.target_id} />
                  <span className="text-sm">{bound.target_label}</span>
                </div>
              ) : (
                <div className="flex items-center gap-2 flex-wrap">
                  {statusRows.map((r) => (
                    <Button
                      key={r.target_id}
                      variant="outline"
                      size="sm"
                      className="gap-1.5"
                      disabled={!!r.identity_name && r.identity_name !== identityName}
                      onClick={() => handleAssign(r.target_id)}
                    >
                      <TargetIcon targetId={r.target_id} />
                      {r.target_label}
                      {r.identity_name && r.identity_name !== identityName && (
                        <span className="text-muted-foreground/60">({r.identity_name})</span>
                      )}
                    </Button>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Credentials (edit mode only) */}
          {internalMode === "edit" && (
            <div className="glass rounded-xl p-6 border border-white/10 space-y-4 flex-shrink-0">
              <h3 className="text-sm font-medium text-foreground">
                {t("identity.credentials")}
              </h3>
              {credTypes.length === 0 && !addGithubOpen ? (
                <p className="text-xs text-muted-foreground/60">
                  {t("identity.noCredentials")}
                </p>
              ) : (
                <div className="space-y-2">
                  {githubCred && !editGithubOpen && (
                    <div className="flex items-center gap-3 rounded-lg border border-border-default px-3 py-2">
                      <div className="h-8 w-8 flex-shrink-0 rounded-lg flex items-center justify-center border border-border font-semibold text-sm bg-muted text-muted-foreground">
                        {githubCred?.fields.git_name?.charAt(0)?.toUpperCase() ?? "G"}
                      </div>
                      <div className="flex-1 min-w-0">
                        <span className="text-sm font-medium">
                          github-account
                        </span>
                        <p className="text-xs text-muted-foreground truncate">
                          {githubCred.fields.git_name ?? ""}
                          {githubCred.fields.git_email
                            ? ` <${githubCred.fields.git_email}>`
                            : ""}
                        </p>
                      </div>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        onClick={() => setEditGithubOpen(true)}
                      >
                        <Edit3 size={14} />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 hover:text-red-500 hover:bg-red-100 dark:hover:text-red-400 dark:hover:bg-red-500/10"
                        onClick={() => setRemoveCredType("github-account")}
                      >
                        <Trash2 size={14} />
                      </Button>
                    </div>
                  )}
                  {editGithubOpen && (
                    <AddGithubInlineForm
                      identityName={identityName!}
                      initial={{
                        gitName: githubCred?.fields.git_name,
                        gitEmail: githubCred?.fields.git_email,
                        sshKeyPath: githubCred?.fields.ssh_key_path,
                      }}
                      onClose={() => setEditGithubOpen(false)}
                    />
                  )}
                </div>
              )}
              {/* Add credential for unconfigured types */}
              {!hasGithub && !addGithubOpen && (
                <Button
                  variant="outline"
                  size="sm"
                  className="gap-1"
                  onClick={() => setAddGithubOpen(true)}
                >
                  <Plus size={14} />
                  {t("identity.addGithub")}
                </Button>
              )}

              {/* Inline add form */}
              {addGithubOpen && (
                <AddGithubInlineForm
                  identityName={identityName!}
                  onClose={() => setAddGithubOpen(false)}
                />
              )}
            </div>
          )}
        </div>
      </FullScreenPanel>

      {/* Remove credential confirm */}
      {removeCredType && (
        <ConfirmDialog
          isOpen
          variant="destructive"
          title={t("identity.removeTitle")}
          message={t("identity.removeMessage", {
            name: identityName,
            type: removeCredType,
          })}
          confirmText={t("common.confirm")}
          onConfirm={() => handleRemoveCred(removeCredType)}
          onCancel={() => setRemoveCredType(null)}
        />
      )}
    </>
  );
};

export default IdentityEditPanel;
