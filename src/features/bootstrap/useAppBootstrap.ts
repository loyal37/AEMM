import { useQuery } from "@tanstack/react-query";
import { getAppBootstrap } from "../../lib/tauri";

export function useAppBootstrap() {
  return useQuery({
    queryKey: ["app-bootstrap"],
    queryFn: getAppBootstrap,
    retry: false,
  });
}
