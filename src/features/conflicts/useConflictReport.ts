import { useQuery } from "@tanstack/react-query";
import { getActiveConflictReport } from "../../lib/tauri";

export const CONFLICT_REPORT_KEY = ["conflicts", "active-profile"] as const;

export function useConflictReport() {
  return useQuery({
    queryKey: CONFLICT_REPORT_KEY,
    queryFn: getActiveConflictReport,
    retry: false,
    staleTime: 10_000,
  });
}
