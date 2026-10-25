import { useNavigate } from "@tanstack/react-router";
import { useScanSession } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Page } from "@/shared/ui";
import { DiskUsage } from "./DiskUsage";
import { LastScan } from "./LastScan";

/** The page's one action: scan the disk (disabled, saying so, while a scan runs). */
function ScanAction() {
  const { m } = useI18n();
  const session = useScanSession();
  const navigate = useNavigate();
  const running = session.scanning || (session.scan !== null && session.assessment === "pending");
  if (running) {
    return (
      <button type="button" disabled className={buttonClass("primary")}>
        {m.overview.scanning}
      </button>
    );
  }
  return (
    <button
      type="button"
      onClick={() => {
        session.start([]);
        void navigate({ to: "/clean" });
      }}
      className={buttonClass("primary")}
    >
      {m.overview.scan}
    </button>
  );
}

/** The first page: how full the disk is, what the last scan found, and the button to scan. */
export function OverviewPage() {
  const { m } = useI18n();
  return (
    <Page title={m.overview.title} actions={<ScanAction />}>
      <DiskUsage />
      <LastScan />
    </Page>
  );
}
