import React, { useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Edit3, Fingerprint, Plus, ScrollText, Search, Trash2, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ListItemRow } from "@/components/common/ListItemRow";
import { APP_ICON_MAP } from "@/config/appConfig";
import {
  useIdentityList,
  useIdentityStatus,
  useBindIdentityMutation,
  useDeleteIdentityMutation,
  useUnbindIdentityMutation,
} from "@/hooks/useIdentity";
import IdentityEditPanel from "./IdentityEditPanel";
import AuditLogDialog from "./AuditLogDialog";

// 首字母头像颜色（基于名称 hash 选色）
const AVATAR_COLORS = [
  "bg-blue-500/20 text-blue-600 dark:text-blue-400",
  "bg-emerald-500/20 text-emerald-600 dark:text-emerald-400",
  "bg-violet-500/20 text-violet-600 dark:text-violet-400",
  "bg-amber-500/20 text-amber-600 dark:text-amber-400",
  "bg-rose-500/20 text-rose-600 dark:text-rose-400",
  "bg-cyan-500/20 text-cyan-600 dark:text-cyan-400",
];

function avatarClass(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) | 0;
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length];
}

// target_id → app icon key 映射
function targetAppId(targetId: string): string | null {
  if (targetId === "claude-code") return "claude";
  if (targetId.startsWith("hermes:")) return "hermes";
  return null;
}

function TargetIcon({ targetId }: { targetId: string }) {
  const appId = targetAppId(targetId);
  if (!appId || !(appId in APP_ICON_MAP)) return null;
  const cfg = APP_ICON_MAP[appId as keyof typeof APP_ICON_MAP];
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex">{cfg.icon}</span>
      </TooltipTrigger>
      <TooltipContent side="bottom">{cfg.label}</TooltipContent>
    </Tooltip>
  );
}

interface IdentityPanelProps {
  appId?: string;
}

const IdentityPanel: React.FC<IdentityPanelProps> = ({ appId: _appId }) => {
  const { t } = useTranslation();
  const { data: identities = [], isLoading } = useIdentityList();
  const { data: statusRows = [] } = useIdentityStatus();
  const deleteMutation = useDeleteIdentityMutation();
  const bindMutation = useBindIdentityMutation();
  const unbindMutation = useUnbindIdentityMutation();

  const [search, setSearch] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [editingName, setEditingName] = useState<string | null>(null);
  const [auditOpen, setAuditOpen] = useState(false);
  const [assignOpen, setAssignOpen] = useState<string | null>(null); // identity name being assigned

  const filtered = useMemo(() => {
    if (!search.trim()) return identities;
    const q = search.toLowerCase();
    return identities.filter(
      (id) =>
        id.name.toLowerCase().includes(q) ||
        id.description.toLowerCase().includes(q) ||
        id.credentials.some((c) =>
          Object.values(c.fields).some((v) => v.toLowerCase().includes(q)),
        ),
    );
  }, [identities, search]);

  const getBoundTarget = (identityName: string) =>
    statusRows.find((r) => r.identity_name === identityName);

  if (isLoading) {
    return (
      <div className="h-full px-6 py-12 text-center text-muted-foreground">
        {t("common.loading")}
      </div>
    );
  }

  // FullScreenPanel for create or edit
  if (creating || editingName) {
    return (
      <IdentityEditPanel
        mode={creating ? "create" : "edit"}
        identityName={editingName ?? undefined}
        onClose={() => {
          setCreating(false);
          setEditingName(null);
        }}
      />
    );
  }

  const boundCount = statusRows.filter((r) => r.identity_name).length;

  return (
    <TooltipProvider delayDuration={300}>
      <div className="min-h-full px-6 flex flex-col flex-1 min-h-0 overflow-hidden">
        {/* Glass bar */}
        <div className="mb-4 flex flex-shrink-0 items-center gap-4 rounded-xl border border-white/10 px-6 py-4 glass">
          <div className="flex items-center gap-2 min-w-0">
            <Fingerprint className="h-5 w-5 text-muted-foreground shrink-0" />
            <span className="text-sm font-medium text-foreground truncate">
              {t("identity.title")}
            </span>
          </div>
          <Badge variant="outline" className="h-7 shrink-0 whitespace-nowrap bg-background/50 px-3">
            {boundCount}/{statusRows.length}
          </Badge>
          <div className="flex-1" />
          <Button variant="ghost" size="sm" className="gap-1" onClick={() => setCreating(true)}>
            <Plus className="h-4 w-4" />
            {t("identity.create")}
          </Button>
          <Button variant="ghost" size="sm" className="gap-1" onClick={() => setAuditOpen(true)}>
            <ScrollText className="h-4 w-4" />
            {t("identity.audit")}
          </Button>
        </div>

        {/* Search */}
        <div role="search" className="relative flex-shrink-0 mb-4">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-9 pr-9"
            placeholder={t("identity.searchPlaceholder")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search && (
            <button
              className="absolute right-2 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              onClick={() => setSearch("")}
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>

        {/* List */}
        <ScrollArea type="auto" className="-mr-3 flex-1 min-h-0">
          <div className="pb-12 pr-3">
            {filtered.length === 0 ? (
              <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-border p-10 text-center">
                <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-muted">
                  <Plus className="h-7 w-7 text-muted-foreground" />
                </div>
                <h3 className="text-lg font-semibold">
                  {search ? t("identity.noSearchResults") : t("identity.empty")}
                </h3>
                {!search && (
                  <Button variant="outline" className="mt-6" onClick={() => setCreating(true)}>
                    <Plus className="mr-2 h-4 w-4" />
                    {t("identity.create")}
                  </Button>
                )}
              </div>
            ) : (
              <div className="rounded-xl border border-border-default">
                {filtered.map((identity, i) => {
                  const bound = getBoundTarget(identity.name);
                  const credTypes = identity.credentials.map((c) => c.credential_type);
                  const hasGithub = credTypes.includes("github-account");

                  return (
                    <ListItemRow key={identity.id} isLast={i === filtered.length - 1}>
                      {/* Avatar */}
                      <div className={`h-8 w-8 flex-shrink-0 rounded-lg flex items-center justify-center border border-border font-semibold text-sm ${avatarClass(identity.name)}`}>
                        {identity.name.charAt(0).toUpperCase()}
                      </div>

                      {/* Info */}
                      <div className="flex-1 min-w-0">
                        <span className="font-medium text-sm text-foreground truncate">
                          {identity.name}
                        </span>
                        <div className="flex items-center gap-1 mt-0.5">
                          {identity.description && (
                            <span className="text-xs text-muted-foreground/60 truncate mr-1">
                              {identity.description}
                            </span>
                          )}
                          {hasGithub && (
                            <Badge variant="secondary" className="h-5 px-1.5 text-[10px] font-semibold">
                              github
                            </Badge>
                          )}
                          {credTypes.length === 0 && (
                            <span className="text-xs text-muted-foreground/60">
                              {t("identity.noCredentials")}
                            </span>
                          )}
                        </div>
                      </div>

                      {/* Target assign */}
                      <div className="flex-shrink-0 relative">
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-7 gap-1.5 text-xs"
                          onClick={() =>
                            setAssignOpen(assignOpen === identity.name ? null : identity.name)
                          }
                        >
                          {bound ? (
                            <>
                              <TargetIcon targetId={bound.target_id} />
                              <span className="text-muted-foreground">{bound.target_label}</span>
                            </>
                          ) : (
                            <span className="text-muted-foreground/60">
                              {t("identity.unassigned")}
                            </span>
                          )}
                        </Button>
                        {assignOpen === identity.name && (
                          <div className="absolute right-0 top-full mt-1 z-50 rounded-lg border border-border bg-popover p-1 shadow-md min-w-[180px]">
                            <button
                              className="flex w-full items-center gap-1.5 rounded-sm px-2 py-1.5 text-xs hover:bg-muted"
                              onClick={() => {
                                const bound = getBoundTarget(identity.name);
                                if (bound) unbindMutation.mutateAsync(bound.target_id);
                                setAssignOpen(null);
                              }}
                            >
                              {t("identity.unassigned")}
                            </button>
                            {statusRows.map((r) => (
                              <button
                                key={r.target_id}
                                className="flex w-full items-center gap-1.5 rounded-sm px-2 py-1.5 text-xs hover:bg-muted disabled:opacity-50"
                                disabled={
                                  !!r.identity_name && r.identity_name !== identity.name
                                }
                                onClick={() => {
                                  bindMutation.mutateAsync({
                                    identity: identity.name,
                                    target: r.target_id,
                                  });
                                  setAssignOpen(null);
                                }}
                              >
                                <TargetIcon targetId={r.target_id} />
                                {r.target_label}
                                {r.identity_name &&
                                  r.identity_name !== identity.name &&
                                  ` (${r.identity_name})`}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>

                      {/* Actions — hover reveal */}
                      <div className="flex items-center gap-0.5 flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => setEditingName(identity.name)}
                          title={t("common.edit")}
                        >
                          <Edit3 size={14} />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 hover:text-red-500 hover:bg-red-100 dark:hover:text-red-400 dark:hover:bg-red-500/10"
                          onClick={() => setDeleteTarget(identity.name)}
                          title={t("common.delete")}
                        >
                          <Trash2 size={14} />
                        </Button>
                      </div>
                    </ListItemRow>
                  );
                })}
              </div>
            )}
          </div>
        </ScrollArea>

        {/* Delete confirm */}
        {deleteTarget && (
          <ConfirmDialog
            isOpen
            variant="destructive"
            title={t("identity.deleteTitle")}
            message={t("identity.deleteMessage", { name: deleteTarget })}
            confirmText={t("common.confirm")}
            onConfirm={async () => {
              await deleteMutation.mutateAsync(deleteTarget);
              setDeleteTarget(null);
            }}
            onCancel={() => setDeleteTarget(null)}
          />
        )}

        {/* Audit log */}
        <AuditLogDialog
          open={auditOpen}
          onOpenChange={setAuditOpen}
        />
      </div>
    </TooltipProvider>
  );
};

export default IdentityPanel;
