import { useSessionOperation, useTransport } from "@/shared/api";

/** An explicitly requested update keeps running when Settings is closed. */
export function useInstallUpdate() {
  const transport = useTransport();
  return useSessionOperation<void, void>(["session-operation", "install-update"], async () => {
    await transport.installUpdate?.();
  });
}
