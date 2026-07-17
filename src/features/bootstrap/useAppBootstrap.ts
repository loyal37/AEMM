import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getAppBootstrap, setStoragePaths, updateSettings } from "../../lib/tauri";
import type { AppBootstrap, AppSettings, StorageSettings } from "../../types/app";

export const APP_BOOTSTRAP_KEY = ["app-bootstrap"] as const;

export function useAppBootstrap() {
  return useQuery({
    queryKey: APP_BOOTSTRAP_KEY,
    queryFn: getAppBootstrap,
    retry: false,
  });
}

export function useUpdateAppSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateSettings,
    onSuccess: (settings: AppSettings) => {
      queryClient.setQueryData<AppBootstrap>(APP_BOOTSTRAP_KEY, (bootstrap) =>
        bootstrap ? { ...bootstrap, settings } : bootstrap,
      );
    },
  });
}

export function useSetStoragePaths() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (storage: StorageSettings) => setStoragePaths(storage),
    onSuccess: (settings) => {
      queryClient.setQueryData<AppBootstrap>(APP_BOOTSTRAP_KEY, (bootstrap) =>
        bootstrap ? { ...bootstrap, settings } : bootstrap,
      );
      void queryClient.invalidateQueries({ queryKey: ["mods"] });
    },
  });
}
