import type { UninstallPreviewView, UninstallReasonView } from "@lumen/api-client";
import { useId, useRef } from "react";
import { formatDateTime, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, Dialog, Disclosure, ErrorNotice } from "@/shared/ui";
import type { ReadyPreview } from "../model/move";
import { mayBeRunning, type QuitStage } from "../model/quit";

export function MoveReasons({ reasons }: { readonly reasons: readonly UninstallReasonView[] }) {
  const { m } = useI18n();
  return (
    <ul className="list-disc space-y-1 pl-5">
      {reasons.map((reason) => (
        <li key={reason}>{m.apps.move.reasons[reason]}</li>
      ))}
    </ul>
  );
}

/** The host supplies all facts and the confirmation; paths are display only. */
export function MoveDialog({
  name,
  preview,
  error,
  loading,
  quit = "none",
  onQuit,
  onRecheck,
  onClose,
  onConfirm,
}: {
  readonly name: string;
  readonly preview: UninstallPreviewView | null;
  readonly error: unknown;
  readonly loading: boolean;
  /** Where asking the app to quit stands; `none` hides it. */
  readonly quit?: QuitStage;
  readonly onQuit?: () => void;
  /** Checks the app again, for one that runs or may run. */
  readonly onRecheck?: () => void;
  readonly onClose: () => void;
  readonly onConfirm: (preview: ReadyPreview) => void;
}) {
  const { locale, m } = useI18n();
  const a = m.apps.move;
  const heading = useId();
  const close = useRef<HTMLButtonElement>(null);
  const app = preview?.kind === "ready" || preview?.kind === "refused" ? preview.app : null;
  const ready = preview?.kind === "ready" ? preview : null;
  const quitText = () => {
    switch (quit) {
      case "none":
        return null;
      case "offer":
        return <p>{a.quitHint}</p>;
      case "waiting":
        return <p role="status">{a.quitAsked}</p>;
      case "still_running":
        return <p>{a.stillRunning}</p>;
      case "itself":
        return <p>{a.quitItself}</p>;
      case "failed":
        return <p>{a.quitFailed}</p>;
    }
  };
  const message = () => {
    if (preview === null) return null;
    switch (preview.kind) {
      case "ready":
        return null;
      case "refused":
        return (
          <>
            <p className="font-medium">{a.refused}</p>
            <MoveReasons reasons={preview.reasons} />
          </>
        );
      case "not_found":
        return <p>{a.notFound}</p>;
      case "ambiguous":
        return <p>{a.ambiguous}</p>;
      case "unsupported":
        return <p>{a.unsupported}</p>;
    }
  };
  return (
    <Dialog open onClose={onClose} labelledBy={heading} initialFocus={close}>
      <h2 id={heading} className="break-words text-title">
        {preview === null || preview.kind === "ready"
          ? a.title(app?.name ?? name)
          : (app?.name ?? name)}
      </h2>
      <div className="mt-4 space-y-3 text-[14px] leading-relaxed text-ink-2">
        {loading && <p role="status">{a.checking}</p>}
        {app !== null && (
          <>
            <dl className="space-y-2 border-y border-line py-3">
              <div>
                <dt className="sr-only">{m.quarantine.columns.location}</dt>
                <dd className="break-all font-mono text-[12px] select-text">{app.path}</dd>
              </div>
              <div>
                <dt className="sr-only">{m.common.details}</dt>
                <dd className="break-all text-[12px]">
                  {[app.bundle_id, app.version].filter(Boolean).join(" · ")}
                </dd>
              </div>
              <div className="flex justify-between gap-3">
                <dt>{a.size}</dt>
                <dd className="tabular-nums">
                  {app.size_complete
                    ? formatBytes(app.size_bytes)
                    : a.atLeast(formatBytes(app.size_bytes))}
                </dd>
              </div>
            </dl>
            {ready !== null && (
              <>
                {app.notes.map((note) => (
                  <p key={note}>{a.notes[note]}</p>
                ))}
                <p>{a.noSpace}</p>
                <Disclosure summary={<span className="text-[12px]">{m.common.details}</span>}>
                  <p className="text-[12px]">
                    {a.expires(
                      formatDateTime(
                        locale,
                        new Date(Date.parse(ready.created_at) + 86_400_000).toISOString(),
                      ),
                    )}
                  </p>
                  <p className="break-all font-mono text-[11px] select-text">
                    {m.confirm.planHash}: {ready.hash}
                  </p>
                </Disclosure>
              </>
            )}
          </>
        )}
        {message()}
        {quitText()}
        {error !== null && <ErrorNotice message={a.failed} error={error} />}
      </div>
      <div className="mt-6 flex flex-col-reverse gap-2 min-[480px]:flex-row min-[480px]:justify-end">
        <button ref={close} type="button" onClick={onClose} className={buttonClass("secondary")}>
          {m.common.close}
        </button>
        {onRecheck !== undefined && mayBeRunning(preview) && quit !== "waiting" && (
          <button
            type="button"
            onClick={onRecheck}
            disabled={loading}
            className={buttonClass("secondary")}
          >
            {a.recheck}
          </button>
        )}
        {onQuit !== undefined && quit === "offer" && (
          <button type="button" onClick={onQuit} className={buttonClass("primary")}>
            {a.quit}
          </button>
        )}
        {ready !== null && (
          <button type="button" onClick={() => onConfirm(ready)} className={buttonClass("primary")}>
            {m.confirm.move}
          </button>
        )}
      </div>
    </Dialog>
  );
}
