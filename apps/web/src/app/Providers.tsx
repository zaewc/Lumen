import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { type LumenTransport, TransportContext } from "@/shared/api";
import { I18nProvider } from "@/shared/i18n";

const queries = new QueryClient();

/** The language, transport and query cache every page uses. */
export function Providers({
  transport,
  children,
}: {
  readonly transport: LumenTransport;
  readonly children: ReactNode;
}) {
  return (
    <I18nProvider>
      <TransportContext value={transport}>
        <QueryClientProvider client={queries}>{children}</QueryClientProvider>
      </TransportContext>
    </I18nProvider>
  );
}
