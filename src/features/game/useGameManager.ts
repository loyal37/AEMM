import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  detectGameInstallations,
  getGameStatus,
  launchGame,
  openGameDirectory,
  setEfmiLoaderRoot,
  setGameInstallation,
  setGameLaunchMode,
} from "../../lib/tauri";

const GAME_STATUS_KEY = ["game-status"] as const;

function useRefreshGameState() {
  const queryClient = useQueryClient();
  return async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: GAME_STATUS_KEY }),
      queryClient.invalidateQueries({ queryKey: ["app-bootstrap"] }),
    ]);
  };
}

export function useGameStatus() {
  return useQuery({
    queryKey: GAME_STATUS_KEY,
    queryFn: getGameStatus,
    retry: false,
  });
}

export function useDetectGameInstallations() {
  return useMutation({ mutationFn: detectGameInstallations });
}

export function useSetGameInstallation() {
  const refresh = useRefreshGameState();
  return useMutation({
    mutationFn: setGameInstallation,
    onSuccess: refresh,
  });
}

export function useSetEfmiLoaderRoot() {
  const refresh = useRefreshGameState();
  return useMutation({
    mutationFn: setEfmiLoaderRoot,
    onSuccess: refresh,
  });
}

export function useSetGameLaunchMode() {
  const refresh = useRefreshGameState();
  return useMutation({
    mutationFn: setGameLaunchMode,
    onSuccess: refresh,
  });
}

export function useOpenGameDirectory() {
  return useMutation({ mutationFn: openGameDirectory });
}

export function useLaunchGame() {
  return useMutation({ mutationFn: launchGame });
}
