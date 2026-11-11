import type { FailureView, UninstallOutcomeView, UninstallPreviewView } from "@lumen/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { MOVING_OPERATIONS, QUERY_KEYS, useSessionOperation, useTransport } from "@/shared/api";
import type { Messages } from "@/shared/i18n";

export type ReadyPreview = Extract<UninstallPreviewView, { kind: "ready" }>;

/** Confirmation stays bound to the preview even if the Apps page is left. */
export function useAppMove() {
  const transport = useTransport();
  const queries = useQueryClient();
  return useSessionOperation(
    MOVING_OPERATIONS.app,
    async (preview: ReadyPreview): Promise<UninstallOutcomeView> => {
      if (transport.uninstall === undefined) throw new Error("unsupported");
      try {
        return await transport.uninstall(preview.preview, preview.hash);
      } finally {
        // Even an error may follow a move. Refresh the actual host records.
        await Promise.all([
          queries.invalidateQueries({ queryKey: QUERY_KEYS.held }),
          queries.invalidateQueries({ queryKey: QUERY_KEYS.disk }),
          queries.invalidateQueries({ queryKey: ["software"] }),
        ]);
      }
    },
  );
}

/** Why the executor did not move the app, in the clean-up's words. */
export function failureText(m: Messages, reason: FailureView): string {
  switch (reason) {
    case "identity_mismatch":
      return m.item.failIdentityMismatch;
    case "in_use":
      return m.item.failInUse;
    case "volume_unsupported":
      return m.item.failVolumeUnsupported;
    case "store_unavailable":
      return m.item.failStoreUnavailable;
    case "move_failed":
      return m.item.failMoveFailed;
    case "verification_failed":
      return m.item.failVerificationFailed;
    case "other":
      return m.item.failOther;
  }
}
