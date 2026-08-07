import React from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useIdentityAudit } from "@/hooks/useIdentity";

interface AuditLogDialogProps {
  open: boolean;
  identityName?: string;
  onOpenChange: (open: boolean) => void;
}

const AuditLogDialog: React.FC<AuditLogDialogProps> = ({
  open,
  identityName,
  onOpenChange,
}) => {
  const { t } = useTranslation();
  const { data: rows = [], isLoading } = useIdentityAudit({
    identity: identityName,
    limit: 100,
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogClose className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2">
          <X className="h-4 w-4" />
          <span className="sr-only">Close</span>
        </DialogClose>
        <DialogHeader>
          <DialogTitle>
            {t("identity.auditTitle")}
            {identityName ? ` - ${identityName}` : ""}
          </DialogTitle>
        </DialogHeader>
        <ScrollArea className="max-h-[60vh]">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("identity.timestamp")}</TableHead>
                <TableHead>{t("identity.action")}</TableHead>
                <TableHead>{t("identity.boundIdentity")}</TableHead>
                <TableHead>{t("identity.target")}</TableHead>
                <TableHead>{t("identity.result")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={5}
                    className="py-8 text-center text-muted-foreground"
                  >
                    {isLoading ? t("common.loading") : t("identity.noAudit")}
                  </TableCell>
                </TableRow>
              ) : (
                rows.map((row, index) => (
                  <TableRow key={`${row.timestamp}-${index}`}>
                    <TableCell className="whitespace-nowrap text-xs text-muted-foreground">
                      {row.timestamp}
                    </TableCell>
                    <TableCell>{row.action}</TableCell>
                    <TableCell>{row.identity_name ?? "-"}</TableCell>
                    <TableCell>{row.target_id ?? "-"}</TableCell>
                    <TableCell>{row.result}</TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
};

export default AuditLogDialog;
