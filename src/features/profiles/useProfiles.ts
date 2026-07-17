import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  copyProfile,
  createProfile,
  deleteProfile,
  listProfiles,
  renameProfile,
  reorderProfileMods,
  switchProfile,
} from "../../lib/tauri";
import { CONFLICT_REPORT_KEY } from "../conflicts/useConflictReport";

export const PROFILE_LIST_KEY = ["profiles", "list"] as const;

export function useProfiles() {
  return useQuery({
    queryKey: PROFILE_LIST_KEY,
    queryFn: listProfiles,
    retry: false,
  });
}

function useRefreshProfiles() {
  const queryClient = useQueryClient();
  return async () => {
    await queryClient.invalidateQueries({ queryKey: PROFILE_LIST_KEY });
  };
}

export function useCreateProfile() {
  const refresh = useRefreshProfiles();
  return useMutation({ mutationFn: createProfile, onSuccess: refresh });
}

export function useRenameProfile() {
  const refresh = useRefreshProfiles();
  return useMutation({
    mutationFn: ({ profileId, name }: { profileId: string; name: string }) =>
      renameProfile(profileId, name),
    onSuccess: refresh,
  });
}

export function useCopyProfile() {
  const refresh = useRefreshProfiles();
  return useMutation({
    mutationFn: ({ sourceProfileId, name }: { sourceProfileId: string; name: string }) =>
      copyProfile(sourceProfileId, name),
    onSuccess: refresh,
  });
}

export function useDeleteProfile() {
  const refresh = useRefreshProfiles();
  return useMutation({ mutationFn: deleteProfile, onSuccess: refresh });
}

export function useReorderProfileMods() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ profileId, modIds }: { profileId: string; modIds: string[] }) =>
      reorderProfileMods(profileId, modIds),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: PROFILE_LIST_KEY }),
        queryClient.invalidateQueries({ queryKey: CONFLICT_REPORT_KEY }),
      ]);
    },
  });
}

export function useSwitchProfile() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: switchProfile,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: PROFILE_LIST_KEY }),
        queryClient.invalidateQueries({ queryKey: ["mods"] }),
        queryClient.invalidateQueries({ queryKey: CONFLICT_REPORT_KEY }),
      ]);
    },
  });
}
