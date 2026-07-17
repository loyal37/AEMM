import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getAppBootstrap, updateSettings } from "../../lib/tauri";
import type { AppBootstrap, AppSettings } from "../../types/app";

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
