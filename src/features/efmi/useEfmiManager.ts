import { useMutation, useQueryClient } from "@tanstack/react-query";
import { setEfmiModsDirectory } from "../../lib/tauri";

export function useSetEfmiModsDirectory() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setEfmiModsDirectory,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["app-bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["mods"] }),
        queryClient.invalidateQueries({ queryKey: ["profiles"] }),
        queryClient.invalidateQueries({ queryKey: ["conflicts"] }),
      ]);
    },
  });
}
