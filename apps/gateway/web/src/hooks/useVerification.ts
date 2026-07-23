import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { getRecordCbor } from "../api.js";
import { verifyRecordById, type VerificationResult } from "../lib/verify.js";

/**
 * Real client-side verification of a record, wired into TanStack Query
 * so watch pages get loading/error semantics for free. The kernel
 * (`@evermesh/kernel`, WASM) is imported dynamically so it's only
 * fetched/instantiated on pages that actually verify something.
 */
export function useVerification(recordId: string | undefined): UseQueryResult<VerificationResult> {
  return useQuery({
    queryKey: ["verify", recordId],
    queryFn: async () => {
      if (!recordId) throw new Error("no record id");
      const kernel = await import("@evermesh/kernel");
      return verifyRecordById(
        recordId,
        async (id) => getRecordCbor(id),
        { verifyRecord: kernel.verifyRecord, deriveId: kernel.deriveId },
      );
    },
    enabled: Boolean(recordId),
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
