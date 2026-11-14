import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactNode, useEffect } from "react";
import { ScanSessionProvider } from "@/entities/scan-session";
import { type LumenTransport, TransportContext } from "@/shared/api";
import { I18nProvider } from "@/shared/i18n";
import { ThemeProvider } from "@/shared/theme";
import { followUpdateFocus } from "./updateFocus";

const queries = new QueryClient();

/** The language, transport, query cache and scan session every page uses. */
export function Providers({
  transport,
  children,
}: {
  readonly transport: LumenTransport;
  readonly children: ReactNode;
}) {
  useEffect(() => followUpdateFocus(queries, window), []);
  return (
    <I18nProvider>
      <TransportContext value={transport}>
        <QueryClientProvider client={queries}>
          <ThemeProvider setNativeTheme={transport.setTheme}>
            <ScanSessionProvider>{children}</ScanSessionProvider>
          </ThemeProvider>
        </QueryClientProvider>
      </TransportContext>
    </I18nProvider>
  );
}
