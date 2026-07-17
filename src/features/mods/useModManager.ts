import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelModImport,
  commitModImport,
  getModDetails,
  getModPreview,
  listInstalledMods,
  openModDirectory,
  prepareModImport,
  scanModRepository,
  setModFavorite,
  setModsEnabled,
  updateLocalModMetadata,
} from "../../lib/tauri";
import type { LocalModMetadata, ModDetails, ModListItem } from "../../types/app";
import { CONFLICT_REPORT_KEY } from "../conflicts/useConflictReport";

export const MOD_LIST_KEY = ["mods", "list"] as const;
const modDetailsKey = (modId: string) => ["mods", "details", modId] as const;

export function useInstalledMods() {
  return useQuery({
    queryKey: MOD_LIST_KEY,
    queryFn: listInstalledMods,
    retry: false,
  });
}

export function useScanMods() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: scanModRepository,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["mods"] });
      await queryClient.invalidateQueries({ queryKey: CONFLICT_REPORT_KEY });
    },
  });
}

export function usePrepareModImport() {
  return useMutation({ mutationFn: prepareModImport });
}

export function useCommitModImport() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: commitModImport,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["mods"] });
      await queryClient.invalidateQueries({ queryKey: CONFLICT_REPORT_KEY });
    },
  });
}

export function useCancelModImport() {
  return useMutation({ mutationFn: cancelModImport });
}

export function useModDetails(modId: string | undefined) {
  return useQuery({
    queryKey: modDetailsKey(modId ?? "missing"),
    queryFn: () => getModDetails(modId ?? ""),
    enabled: Boolean(modId),
    retry: false,
  });
}

export function useModPreview(modId: string, enabled: boolean) {
  return useQuery({
    queryKey: ["mods", "preview", modId],
    queryFn: () => getModPreview(modId),
    enabled,
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30_000,
  });
}

export function useSetModFavorite() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ modIds, favorite }: { modIds: string[]; favorite: boolean }) =>
      setModFavorite(modIds, favorite),
    onSuccess: (_result, variables) => {
      const ids = new Set(variables.modIds);
      queryClient.setQueryData<ModListItem[]>(MOD_LIST_KEY, (items) =>
        items?.map((item) =>
          ids.has(item.id) ? { ...item, favorite: variables.favorite } : item,
        ),
      );
      for (const modId of variables.modIds) {
        queryClient.setQueryData<ModDetails>(modDetailsKey(modId), (details) =>
          details
            ? {
                ...details,
                item: { ...details.item, favorite: variables.favorite },
                localMetadata: { ...details.localMetadata, favorite: variables.favorite },
              }
            : details,
        );
      }
    },
  });
}

export function useSetModsEnabled() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ modIds, enabled }: { modIds: string[]; enabled: boolean }) =>
      setModsEnabled(modIds, enabled),
    onSuccess: (result, variables) => {
      const ids = new Set(variables.modIds);
      queryClient.setQueryData<ModListItem[]>(MOD_LIST_KEY, (items) =>
        items?.map((item) =>
          ids.has(item.id) ? { ...item, enabled: variables.enabled } : item,
        ),
      );
      for (const modId of variables.modIds) {
        queryClient.setQueryData<ModDetails>(modDetailsKey(modId), (details) =>
          details
            ? { ...details, item: { ...details.item, enabled: variables.enabled } }
            : details,
        );
      }
      if (result.updated !== variables.modIds.length) {
        void queryClient.invalidateQueries({ queryKey: ["mods"] });
      }
      void queryClient.invalidateQueries({ queryKey: CONFLICT_REPORT_KEY });
    },
  });
}

export function useUpdateLocalModMetadata(modId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (metadata: LocalModMetadata) => updateLocalModMetadata(modId, metadata),
    onSuccess: (updated) => {
      queryClient.setQueryData<ModListItem[]>(MOD_LIST_KEY, (items) =>
        items?.map((item) => (item.id === updated.id ? updated : item)),
      );
      void queryClient.invalidateQueries({ queryKey: modDetailsKey(modId) });
      void queryClient.invalidateQueries({ queryKey: CONFLICT_REPORT_KEY });
    },
  });
}

export function useOpenModDirectory() {
  return useMutation({ mutationFn: openModDirectory });
}
