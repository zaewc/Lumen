import { type HelperAction, helperActions, helperLine, useHelper } from "@/entities/helper";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice, GroupRow } from "@/shared/ui";

/**
 * Lumen's administrator helper (ADR-0063): off until the person turns it on
 * (first-launch setup or here, ADR-0067), then allowed by an administrator
 * in System Settings › Login Items. Shown only where the host has one (macOS).
 */
export function HelperSetting() {
  const { m } = useI18n();
  const h = m.settings.permissions.helper;
  const { supported, state, change } = useHelper();
  if (!supported || state.data === undefined) {
    return null;
  }
  const line = helperLine(state.data);
  if (line === null) {
    return null;
  }
  const status = (() => {
    switch (line.kind) {
      case "off":
        return h.off;
      case "waiting":
        return h.waiting;
      case "on":
        return h.on(line.version);
      case "problem":
        return h.problems[line.problem];
    }
  })();
  const label: Record<HelperAction, string> = {
    enable: h.enable,
    disable: h.disable,
    open_login_items: h.openLoginItems,
  };
  return (
    <GroupRow
      title={h.title}
      description={
        <>
          <p>{h.intro}</p>
          <p className="mt-1">{h.scope}</p>
          <p className="mt-1 font-medium text-ink" aria-live="polite">
            {status}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            {helperActions(state.data).map((action, index) => (
              <button
                key={action}
                type="button"
                disabled={change.isPending}
                onClick={() => change.mutate(action)}
                className={buttonClass(index === 0 ? "primary" : "secondary", "sm")}
              >
                {label[action]}
              </button>
            ))}
          </div>
          {change.isError && <ErrorNotice message={h.changeFailed} error={change.error} />}
        </>
      }
    />
  );
}
