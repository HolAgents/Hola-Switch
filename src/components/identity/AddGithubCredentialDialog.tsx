import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Copy, ExternalLink, Eye, EyeOff, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAddGithubCredentialMutation } from "@/hooks/useIdentity";
import type { CredentialAddResult } from "@/lib/api/identity";

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

interface AddGithubCredentialDialogProps {
  open: boolean;
  identityName: string;
  onOpenChange: (open: boolean) => void;
}

const AddGithubCredentialDialog: React.FC<AddGithubCredentialDialogProps> = ({
  open,
  identityName,
  onOpenChange,
}) => {
  const { t } = useTranslation();
  const addMutation = useAddGithubCredentialMutation();

  const [token, setToken] = useState("");
  const [gitName, setGitName] = useState("");
  const [gitEmail, setGitEmail] = useState("");
  const [sshKeyPath, setSshKeyPath] = useState("");
  const [showToken, setShowToken] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  // 每次打开时重置表单
  useEffect(() => {
    if (open) {
      setToken("");
      setGitName("");
      setGitEmail("");
      setSshKeyPath("");
      setAdvancedOpen(false);
    }
  }, [open]);

  const canSubmit =
    token.trim() !== "" &&
    gitName.trim() !== "" &&
    EMAIL_PATTERN.test(gitEmail.trim()) &&
    !addMutation.isPending;

  const handleCopyKey = useCallback((key: string) => {
    navigator.clipboard.writeText(key).catch(() => {});
  }, []);

  const showSshWarning = useCallback((result: CredentialAddResult) => {
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
            onClick={() => result.ssh_public_key && handleCopyKey(result.ssh_public_key)}
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
  }, [t, handleCopyKey]);

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
      onOpenChange(false);
      if (!result.ssh_key_uploaded) {
        // 延迟一下让 dialog 关闭动画先跑完
        setTimeout(() => showSshWarning(result), 200);
      }
    } catch {
      // mutation onError already surfaces a toast
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !addMutation.isPending) onOpenChange(false);
      }}
    >
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("identity.addGithub")}</DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label>{t("identity.token")}</Label>
            <div className="relative">
              <Input
                type={showToken ? "text" : "password"}
                value={token}
                autoComplete="off"
                className="pr-9"
                onChange={(event) => setToken(event.target.value)}
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
            <p className="text-xs text-muted-foreground">
              {t("identity.maskedHint")}
            </p>
          </div>
          <div className="space-y-1.5">
            <Label>{t("identity.gitName")}</Label>
            <Input
              value={gitName}
              onChange={(event) => setGitName(event.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label>{t("identity.gitEmail")}</Label>
            <Input
              type="email"
              value={gitEmail}
              onChange={(event) => setGitEmail(event.target.value)}
            />
          </div>

          <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
            <CollapsibleTrigger asChild>
              <Button variant="ghost" size="sm" className="flex items-center gap-1 px-0">
                <ChevronDown
                  className={`h-4 w-4 transition-transform ${advancedOpen ? "rotate-0" : "-rotate-90"}`}
                />
                {t("identity.sshKeyAdvanced")}
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="space-y-1.5 pt-1">
              <Label className="text-xs">{t("identity.sshKeyPath")}</Label>
              <Input
                value={sshKeyPath}
                placeholder={t("identity.sshKeyPathPlaceholder")}
                onChange={(event) => setSshKeyPath(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {t("identity.sshKeyPathHint")}
              </p>
            </CollapsibleContent>
          </Collapsible>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={addMutation.isPending}
          >
            {t("common.cancel")}
          </Button>
          <Button onClick={() => void handleSubmit()} disabled={!canSubmit}>
            {addMutation.isPending ? (
              <>
                <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
                {t("identity.tokenValidating")}
              </>
            ) : (
              t("common.confirm")
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default AddGithubCredentialDialog;
