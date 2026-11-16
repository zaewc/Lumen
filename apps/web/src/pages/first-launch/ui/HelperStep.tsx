import { useEffect } from "react";
import { helperLine, useHelper } from "@/entities/helper";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { StepHeading } from "./StepHeading";

/**
 * The administrator helper (ADR-0063), offered on first launch with turning
 * it on as the main choice (ADR-0067). Turning it on registers it; macOS
 * then asks an administrator to allow it in Login Items, so the step waits
 * for that and moves on when the person does. Skipped where there is none.
 */
export function HelperStep({
  titleId,
  onDone,
}: {
  readonly titleId: string;
  readonly onDone: () => void;
}) {
  const { m } = useI18n();
  const h = m.settings.permissions.helper;
  const s = m.setup;
  const { state, change } = useHelper();
  const line = state.data === undefined ? undefined : helperLine(state.data);
  // A host without a helper (off macOS) has nothing to set up.
  useEffect(() => {
    if (line === null) onDone();
  }, [line, onDone]);
  const status = (() => {
    switch (line?.kind) {
      case undefined:
        return state.isPending ? m.common.loading : null;
      case "off":
        return null;
      case "waiting":
        return h.waiting;
      case "on":
        return h.on(line.version);
      case "problem":
        return h.problems[line.problem];
    }
  })();
  const primary = (() => {
    switch (line?.kind) {
      case "off":
      case "problem":
        return { label: h.enable, run: () => change.mutate("enable") };
      case "waiting":
        return { label: h.openLoginItems, run: () => change.mutate("open_login_items") };
      case "on":
        return { label: s.next, run: onDone };
      case undefined:
        return null;
    }
  })();
  return (
    <>
      <StepHeading id={titleId} title={s.helper.title} intro={h.intro} />
      <p className="mt-2 text-ink-2">{s.helper.approval}</p>
      <p className="mt-2 text-[13px] text-ink-3">{h.notYet}</p>
      {status !== null && (
        <p className="mt-5 font-medium text-ink" aria-live="polite">
          {status}
        </p>
      )}
      {state.isError && (
        <div className="mt-4">
          <ErrorNotice message={h.changeFailed} error={state.error} />
        </div>
      )}
      {change.isError && (
        <div className="mt-4">
          <ErrorNotice message={h.changeFailed} error={change.error} />
        </div>
      )}
      <div className="mt-8 grid gap-2 min-[480px]:grid-cols-2">
        {line?.kind !== "on" && (
          <button
            type="button"
            onClick={onDone}
            disabled={change.isPending}
            className={buttonClass("secondary", "lg")}
          >
            {line?.kind === "waiting" ? s.next : s.later}
          </button>
        )}
        {primary !== null && (
          <button
            type="button"
            onClick={primary.run}
            disabled={change.isPending}
            className={buttonClass("primary", "lg")}
          >
            {primary.label}
          </button>
        )}
      </div>
    </>
  );
}
