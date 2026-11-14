import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactNode, useEffect } from "react";
import { ScanSessionProvider } from "@/entities/scan-session";
import { type LumenTransport, TransportContext } from "@/shared/api";
import { I18nProvider, useI18n } from "@/shared/i18n";
import { ThemeProvider } from "@/shared/theme";
import { followUpdateFocus } from "./updateFocus";

const queries = new QueryClient();

/**
 * Tells the desktop host the page's language, for its own confirmation
 * dialogs (FS-1). A host without the command keeps them in English.
 */
function HostLanguage({ transport }: { readonly transport: LumenTransport }) {
  const { locale } = useI18n();
  useEffect(() => {
    transport.setLanguage?.(locale).catch(() => undefined);
  }, [transport, locale]);
  return null;
}

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
        <HostLanguage transport={transport} />
        <QueryClientProvider client={queries}>
          <ThemeProvider setNativeTheme={transport.setTheme}>
            <ScanSessionProvider>{children}</ScanSessionProvider>
          </ThemeProvider>
        </QueryClientProvider>
      </TransportContext>
    </I18nProvider>
  );
}
