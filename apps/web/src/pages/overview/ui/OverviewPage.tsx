import { useI18n } from "@/shared/i18n";
import { Page } from "@/shared/ui";
import { DiskCard } from "./DiskCard";
import { SessionSummary } from "./SessionSummary";

/** The first page: how full the disk is, and what the last scan found. */
export function OverviewPage() {
  const { m } = useI18n();
  return (
    <Page title={m.overview.title}>
      <DiskCard />
      <SessionSummary />
    </Page>
  );
}
