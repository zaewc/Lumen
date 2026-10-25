import { useNavigate, useSearch } from "@tanstack/react-router";
import { type ReactNode, useState } from "react";
import { summarize, useScanSession } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { shortcutProps, useShortcut } from "@/shared/lib";
import { buttonClass, Disclosure, Page } from "@/shared/ui";
import type { Filter } from "../model/filter";
import { cleanUpStep } from "../model/step";
import { useCleanUpAction } from "./CleanUpAction";
import { DonePanel } from "./DonePanel";
import { ProgressPanel } from "./ProgressPanel";
import { Results } from "./Results";
import { StartPanel } from "./StartPanel";
import { StepHeading } from "./StepHeading";

/**
 * The guided clean-up: scan (read-only), let the policy decide, then move
 * what it proposes to quarantine after one explicit confirmation (ADR-0034).
 * Nothing is deleted here; everything moved can be restored. The scan
 * belongs to the session, so it goes on while another page is open. The
 * toolbar holds each step's action; ⌘R (Ctrl+R) scans again once there is
 * something to scan again.
 */
export function CleanUpPage() {
  const { m } = useI18n();
  const session = useScanSession();
  const navigate = useNavigate();
  const { show } = useSearch({ from: "/clean" });
  const filter: Filter = show ?? "all";
  const [folders, setFolders] = useState("");
  // Focus follows the flow once the user has started it (here, or on Overview).
  const [acted, setActed] = useState(() => session.scanning);

  const start = (roots: string[]) => {
    setActed(true);
    session.start(roots);
  };
  const rescan = () => {
    setActed(true);
    session.rescan();
  };
  const startOver = () => {
    setActed(true);
    session.startOver();
  };

  const step = cleanUpStep({
    scanning: session.scanning,
    scanned: session.scan !== null,
    assessment: session.assessment,
    done: session.report !== null,
  });
  const summary = session.candidates === null ? null : summarize(session.candidates);
  const cleanUp = useCleanUpAction({
    proposed: summary?.quarantine.items.length ?? 0,
    onDone: (report) => {
      setActed(true);
      session.finish(report);
    },
  });
  const canRescan = step === "results" || step === "done" || step === "decideFailed";
  useShortcut("r", rescan, canRescan);

  const scanAgain = (
    <button
      type="button"
      onClick={rescan}
      {...shortcutProps(m.common.scanAgain, "r")}
      className={buttonClass("secondary")}
    >
      {m.common.scanAgain}
    </button>
  );

  const actions = (): ReactNode => {
    switch (step) {
      case "start":
        return (
          <button type="button" onClick={() => start([])} className={buttonClass("primary")}>
            {m.common.scanDisk}
          </button>
        );
      case "scanning":
      case "deciding":
        return (
          <button type="button" disabled className={buttonClass("primary")}>
            {m.common.scanning}
          </button>
        );
      case "decideFailed":
      case "done":
        return scanAgain;
      case "results":
        return (
          <>
            {scanAgain}
            {cleanUp.button}
          </>
        );
    }
  };

  const view = (): ReactNode => {
    switch (step) {
      case "start":
        return (
          <StartPanel
            folders={folders}
            onFoldersChange={setFolders}
            onScan={start}
            error={session.scanError}
            focus={acted}
          />
        );
      case "scanning":
      case "deciding":
        return (
          <ProgressPanel progress={session.progress} deciding={step === "deciding"} focus={acted} />
        );
      case "decideFailed":
        return (
          <section className="space-y-3">
            <StepHeading focus={acted} className="text-[15px] font-semibold text-ink">
              {m.scanning.failed}
            </StepHeading>
            <button type="button" onClick={rescan} className={buttonClass("secondary")}>
              {m.common.tryAgain}
            </button>
            <Disclosure
              summary={<span className="text-[12px] text-ink-3">{m.common.details}</span>}
            >
              <p className="font-mono text-[11px] break-all text-ink-3 select-text">
                {String(session.assessmentError)}
              </p>
            </Disclosure>
          </section>
        );
      case "results":
        return session.scan !== null && session.candidates !== null && summary !== null ? (
          <>
            {cleanUp.error}
            <Results
              scan={session.scan}
              candidates={session.candidates}
              summary={summary}
              filter={filter}
              onFilterChange={(next) => {
                void navigate({
                  to: "/clean",
                  search: next === "all" ? {} : { show: next },
                  replace: true,
                });
              }}
              onStartOver={startOver}
              focus={acted}
            />
            {cleanUp.dialog}
          </>
        ) : null;
      case "done":
        return session.report !== null ? <DonePanel report={session.report} focus={acted} /> : null;
    }
  };

  return (
    <Page title={m.nav.cleanUp} actions={actions()}>
      {view()}
    </Page>
  );
}
