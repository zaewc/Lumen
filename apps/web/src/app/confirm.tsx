import { isTauri } from "@tauri-apps/api/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { confirmHost, type HostAsk } from "@/shared/api";
import { I18nProvider } from "@/shared/i18n";
import { ThemeProvider } from "@/shared/theme";
import { ConfirmWindow } from "./ConfirmWindow";
import "./styles.css";

// The desktop host's confirmation window (FS-1): its own page, opened and
// answered only by the host, apart from the app's window.
const element = document.getElementById("root");
if (element === null) {
  throw new Error("confirm.html has no #root");
}

// In a plain browser (development) the window shows a sample question.
const SAMPLE: HostAsk = {
  kind: "move",
  count: 3,
  total_bytes: "5100000000",
  proposed: {
    items: [
      { path: "/Users/ana/Library/Developer/Xcode/DerivedData", bytes: "4200000000" },
      { path: "/Users/ana/.gradle/caches", bytes: "140000000" },
    ],
    more: 0,
  },
  chosen: [{ path: "/Users/ana/Library/Caches/Google/Chrome", bytes: "760000000" }],
};
const sample: typeof confirmHost = {
  request: () => Promise.resolve(SAMPLE),
  answer: () => Promise.resolve(),
};

createRoot(element).render(
  <StrictMode>
    <I18nProvider>
      <ThemeProvider setNativeTheme={undefined}>
        <ConfirmWindow host={isTauri() ? confirmHost : sample} />
      </ThemeProvider>
    </I18nProvider>
  </StrictMode>,
);
