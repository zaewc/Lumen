import type { CopyRequest } from "@lumen/api-client";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { type ReactNode, useRef, useState } from "react";
import { summarize, useScanSession } from "@/entities/scan-session";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { shortcutProps, useShortcut } from "@/shared/lib";
import { buttonClass, ErrorDetails, Page } from "@/shared/ui";
import type { Filter } from "../model/filter";
import {
  type AppCheck,
  isChecked,
  PROPOSED,
  type Selected,
  type Selection,
  selectedOf,
  toggled,
  withApp,
  withCopy,
} from "../model/selection";
import { cleanUpStep } from "../model/step";
import { BeforeScan } from "./BeforeScan";
import { useCleanUpAction } from "./CleanUpAction";
import { DiskUsage } from "./DiskUsage";
import { DonePanel } from "./DonePanel";
import { Duplicates } from "./Duplicates";
import { ProgressPanel } from "./ProgressPanel";
import { Results } from "./Results";
import { StartPanel } from "./StartPanel";
import { StepHeading } from "./StepHeading";

/**
 * Storage (ADR-0069): the disk, then the guided clean-up: scan (read-only), let the policy decide, then move
 * what it proposes to quarantine after one explicit confirmation (ADR-0034).
 * Nothing is deleted here; everything moved can be restored. The scan
 * belongs to the session, so it goes on while another page is open. The
 * toolbar holds each step's actions: once there is something to scan again,
 * "Scan other folders" goes back to the start and "Scan again" (⌘R, Ctrl+R)
 * scans the same folders.
 */
export function CleanUpPage() {
  const { m } = useI18n();
  const session = useScanSession();
  const navigate = useNavigate();
  const { show } = useSearch({ from: "/" });
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
  // What is checked, for the assessment it was made on: a new one starts
  // from Lumen's proposal again.
  const [checks, setChecks] = useState<{
    readonly of: typeof session.candidates;
    readonly selection: Selection;
  }>({ of: null, selection: PROPOSED });
  // A new assessment starts the items from Lumen's proposal again; the apps
  // and copies checked stay, since their previews do not depend on it.
  const current = (checked: typeof checks, candidates: typeof session.candidates) =>
    checked.of === candidates
      ? checked.selection
      : { ...PROPOSED, apps: checked.selection.apps, copies: checked.selection.copies };
  const selection = current(checks, session.candidates);
  const transport = useTransport();
  // Read when an update applies, not when it was made: a preview answering
  // later must not bring back an earlier assessment's choices.
  const latestCandidates = useRef(session.candidates);
  latestCandidates.current = session.candidates;
  const update = (change: (earlier: Selection) => Selection) =>
    setChecks((earlier) => ({
      of: latestCandidates.current,
      selection: change(current(earlier, latestCandidates.current)),
    }));
  // What was sent to be moved is no longer checked: each preview moves once.
  const forgetSent = (sent: Selected) => {
    const gone = new Set([...sent.apps, ...sent.copies].map((each) => each.preview));
    const sentOff = (check: AppCheck) => check.kind === "ready" && gone.has(check.preview);
    update((earlier) => ({
      ...earlier,
      apps: new Map([...earlier.apps].filter(([, check]) => !sentOff(check))),
      copies: new Map([...earlier.copies].filter(([, copy]) => !sentOff(copy.check))),
    }));
  };
  // Checking an app asks the host for a fresh preview (ADR-0062); only a
  // ready one joins the selection, a refused one says so on its row.
  const toggleApp = (key: string) => {
    if (selection.apps.has(key)) {
      update((earlier) => withApp(earlier, key, undefined));
      return;
    }
    const preview = transport.uninstallPreview;
    if (preview === undefined) return;
    update((earlier) => withApp(earlier, key, { kind: "checking" }));
    const settle = (check: Parameters<typeof withApp>[2]) =>
      update((earlier) =>
        earlier.apps.get(key)?.kind === "checking" ? withApp(earlier, key, check) : earlier,
      );
    preview(key).then(
      (answer) =>
        settle(
          answer.kind === "ready"
            ? {
                kind: "ready",
                preview: answer.preview,
                hash: answer.hash,
                bytes: answer.app.size_bytes,
              }
            : { kind: "refused" },
        ),
      () => settle({ kind: "refused" }),
    );
  };
  // Checking a copy asks the host for its preview, with the copy that stays
  // (ADR-0069); only a ready one joins the selection.
  const toggleCopy = (key: string, request: CopyRequest) => {
    if (selection.copies.has(key)) {
      update((earlier) => withCopy(earlier, key, undefined));
      return;
    }
    const preview = transport.copyPreview;
    if (preview === undefined) return;
    const { stays } = request;
    update((earlier) => withCopy(earlier, key, { check: { kind: "checking" }, stays }));
    const settle = (check: AppCheck) =>
      update((earlier) =>
        earlier.copies.get(key)?.check.kind === "checking"
          ? withCopy(earlier, key, { check, stays })
          : earlier,
      );
    preview(request).then(
      (answer) =>
        settle(
          answer.kind === "ready"
            ? { kind: "ready", preview: answer.preview, hash: answer.hash, bytes: answer.bytes }
            : { kind: "refused" },
        ),
      () => settle({ kind: "refused" }),
    );
  };
  const selected = selectedOf(selection, session.candidates ?? []);
  const cleanUp = useCleanUpAction({
    selected,
    onConfirm: () => setActed(true),
    onSent: forgetSent,
  });
  const canRescan = step === "results" || step === "done" || step === "decideFailed";
  // The checkboxes, for the list before a scan and after it (ADR-0069).
  const check = {
    isChecked: (candidate: Parameters<typeof isChecked>[1]) => isChecked(selection, candidate),
    toggle: (candidate: Parameters<typeof toggled>[1]) =>
      update((earlier) => toggled(earlier, candidate)),
    appState: (key: string) => selection.apps.get(key),
    toggleApp,
  };
  useShortcut("r", rescan, canRescan && !session.cleaning);

  const scanAgain = (
    <>
      <button
        type="button"
        onClick={startOver}
        disabled={session.cleaning}
        className={buttonClass("plain")}
      >
        {m.common.scanOther}
      </button>
      <button
        type="button"
        onClick={rescan}
        disabled={session.cleaning}
        {...shortcutProps(m.common.scanAgain, "r")}
        className={buttonClass("secondary")}
      >
        {m.common.scanAgain}
      </button>
    </>
  );

  const actions = (): ReactNode => {
    switch (step) {
      case "start":
        // An app checked before a scan moves with the one primary action;
        // scanning is then secondary.
        return selected.count > 0 ? (
          <>
            <button type="button" onClick={() => start([])} className={buttonClass("secondary")}>
              {m.common.scanDisk}
            </button>
            {cleanUp.button}
          </>
        ) : (
          <button type="button" onClick={() => start([])} className={buttonClass("primary")}>
            {m.common.scanDisk}
          </button>
        );
      case "scanning":
      case "deciding":
        return (
          <button type="button" disabled className={buttonClass("primary")}>
            {step === "deciding" ? m.scanning.deciding : m.common.scanning}
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

  // What the scan found; after a clean-up, what is left of it.
  const { scan, candidates } = session;
  const results =
    scan !== null && candidates !== null && summary !== null
      ? (cleanedUp: boolean) => (
          <Results
            scan={scan}
            candidates={candidates}
            summary={summary}
            filter={filter}
            onFilterChange={(next) => {
              void navigate({
                to: "/",
                search: next === "all" ? {} : { show: next },
                replace: true,
              });
            }}
            focus={acted && !cleanedUp}
            cleanedUp={cleanedUp}
            check={check}
          />
        )
      : null;

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
          <ProgressPanel
            progress={session.progress}
            assessmentProgress={session.assessmentProgress}
            deciding={step === "deciding"}
            focus={acted}
          />
        );
      case "decideFailed":
        return (
          <section className="space-y-3">
            <StepHeading focus={acted} className="text-section text-ink">
              {m.scanning.failed}
            </StepHeading>
            <button type="button" onClick={rescan} className={buttonClass("secondary")}>
              {m.common.tryAgain}
            </button>
            <ErrorDetails error={session.assessmentError} />
          </section>
        );
      case "results":
        return results === null ? null : (
          <>
            {cleanUp.error}
            {results(false)}
          </>
        );
      case "done":
        return session.report !== null ? (
          <>
            <DonePanel report={session.report} focus={acted} />
            {results?.(true)}
          </>
        ) : null;
    }
  };

  return (
    <Page title={m.nav.overview} actions={actions()}>
      <DiskUsage />
      {view()}
      {scan !== null && !session.scanning && !scan.partial && (
        <Duplicates
          key={scan.scan}
          scanId={scan.scan}
          onCompare={() => update((earlier) => ({ ...earlier, copies: new Map() }))}
          {...(session.report === null && transport.copyPreview !== undefined
            ? {
                check: {
                  state: (key: string) => selection.copies.get(key),
                  toggle: toggleCopy,
                },
              }
            : {})}
        />
      )}
      {step === "start" && <BeforeScan check={check} />}
    </Page>
  );
}
