import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { type LumenTransport, TransportContext } from "@/shared/api";

const queries = new QueryClient();

/** The transport and query cache every page uses. */
export function Providers({
  transport,
  children,
}: {
  readonly transport: LumenTransport;
  readonly children: ReactNode;
}) {
  return (
    <TransportContext value={transport}>
      <QueryClientProvider client={queries}>{children}</QueryClientProvider>
    </TransportContext>
  );
}
