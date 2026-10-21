import { useNavigate, useSearch } from "@tanstack/react-router";
import { type ReactNode, useState } from "react";
import { summarize, useScanSession } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Page } from "@/shared/ui";
import type { Filter } from "../model/filter";
import { cleanUpStep } from "../model/step";
import { ActionBar } from "./ActionBar";
import { DonePanel } from "./DonePanel";
import { ProgressPanel } from "./ProgressPanel";
import { Results } from "./Results";
import { StartPanel } from "./StartPanel";
import { StepHeading } from "./StepHeading";

/**
 * The guided clean-up: scan (read-only), let the policy decide, then move
 * what it proposes to quarantine after one explicit confirmation (ADR-0034).
 * Nothing is deleted here; everything moved can be restored. The scan
 * belongs to the session, so it goes on while another page is open.
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
          <section className="space-y-4 rounded-xl border border-line bg-surface p-6">
            <StepHeading focus={acted} className="text-[17px] font-semibold text-ink">
              {m.scanning.failed}
            </StepHeading>
            <p className="font-mono text-[11px] break-all text-ink-3 select-text">
              {String(session.assessmentError)}
            </p>
            <button type="button" onClick={startOver} className={buttonClass("secondary")}>
              {m.common.scanAgain}
            </button>
          </section>
        );
      case "results":
        return session.scan !== null && session.candidates !== null && summary !== null ? (
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
            focus={acted}
          />
        ) : null;
      case "done":
        return session.report !== null ? (
          <DonePanel report={session.report} onStartOver={startOver} focus={acted} />
        ) : null;
    }
  };

  return (
    <Page
      title={m.nav.cleanUp}
      {...(step === "results"
        ? {
            actions: (
              <button type="button" onClick={startOver} className={buttonClass("secondary")}>
                {m.common.scanAgain}
              </button>
            ),
          }
        : {})}
      {...(step === "results" && summary !== null
        ? {
            footer: (
              <ActionBar
                proposed={summary.quarantine.items.length}
                bytes={summary.quarantine.bytes}
                onDone={(report) => {
                  setActed(true);
                  session.finish(report);
                }}
              />
            ),
          }
        : {})}
    >
      {view()}
    </Page>
  );
}
