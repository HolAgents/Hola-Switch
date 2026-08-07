import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { identityApi, type AuditFilter } from "@/lib/api/identity";
import { extractErrorMessage } from "@/utils/errorUtils";

export const identityKeys = {
  all: ["identity"] as const,
  list: ["identity", "list"] as const,
  status: ["identity", "status"] as const,
  audit: (filter?: AuditFilter) => ["identity", "audit", filter ?? {}] as const,
};

export function invalidateIdentityCaches(queryClient: QueryClient) {
  return queryClient.invalidateQueries({ queryKey: identityKeys.all });
}

export const useIdentityList = () => {
  return useQuery({
    queryKey: identityKeys.list,
    queryFn: () => identityApi.list(),
  });
};

export const useIdentityStatus = () => {
  return useQuery({
    queryKey: identityKeys.status,
    queryFn: () => identityApi.status(),
  });
};

export const useIdentityAudit = (filter?: AuditFilter) => {
  return useQuery({
    queryKey: identityKeys.audit(filter),
    queryFn: () => identityApi.audit(filter),
  });
};

export const useCreateIdentityMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({
      name,
      description,
    }: {
      name: string;
      description?: string;
    }) => identityApi.create(name, description),
    onSuccess: async () => {
      await invalidateIdentityCaches(queryClient);
      toast.success(t("identity.createSuccess"), { closeButton: true });
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(t("identity.createFailed", { detail }), {
        closeButton: true,
      });
    },
  });
};

export const useDeleteIdentityMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: (name: string) => identityApi.remove(name),
    onSuccess: async () => {
      await invalidateIdentityCaches(queryClient);
      toast.success(t("identity.deleteSuccess"), { closeButton: true });
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(t("identity.deleteFailed", { detail }), {
        closeButton: true,
      });
    },
  });
};

export const useAddGithubCredentialMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: (input: {
      identity: string;
      token: string;
      gitName: string;
      gitEmail: string;
      sshKeyPath?: string;
    }) => identityApi.addGithubCredential(input),
    onSuccess: async () => {
      await invalidateIdentityCaches(queryClient);
      toast.success(t("identity.addGithubSuccess"), { closeButton: true });
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(t("identity.addGithubFailed", { detail }), {
        closeButton: true,
      });
    },
  });
};

export const useRemoveCredentialMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({
      identity,
      credentialType,
    }: {
      identity: string;
      credentialType: string;
    }) => identityApi.removeCredential(identity, credentialType),
    onSuccess: async () => {
      await invalidateIdentityCaches(queryClient);
      toast.success(t("identity.removeSuccess"), { closeButton: true });
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(t("identity.removeFailed", { detail }), {
        closeButton: true,
      });
    },
  });
};

export const useBindIdentityMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: ({ identity, target }: { identity: string; target: string }) =>
      identityApi.bind(identity, target),
    onSuccess: async () => {
      await invalidateIdentityCaches(queryClient);
      toast.success(t("identity.bindSuccess"), { closeButton: true });
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(t("identity.bindFailed", { detail }), { closeButton: true });
    },
  });
};

export const useUnbindIdentityMutation = () => {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  return useMutation({
    mutationFn: (target: string) => identityApi.unbind(target),
    onSuccess: async () => {
      await invalidateIdentityCaches(queryClient);
      toast.success(t("identity.unbindSuccess"), { closeButton: true });
    },
    onError: (error: Error) => {
      const detail = extractErrorMessage(error) || t("common.unknown");
      toast.error(t("identity.unbindFailed", { detail }), {
        closeButton: true,
      });
    },
  });
};
