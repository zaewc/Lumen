import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice, GroupRow } from "@/shared/ui";
import { type HelperAction, helperActions, helperLine } from "../model/helper";

const HELPER = ["helper"] as const;

/**
 * Lumen's administrator helper (ADR-0063): off until the person turns it on,
 * then allowed by an administrator in System Settings › Login Items. Shown
 * only where the host has one (macOS). The state is asked again when the
 * window regains focus, since allowing it happens in System Settings.
 */
export function HelperSetting() {
  const { m } = useI18n();
  const h = m.settings.permissions.helper;
  const transport = useTransport();
  const queries = useQueryClient();
  const read = transport.helperState;
  const state = useQuery({
    queryKey: HELPER,
    queryFn: () => (read === undefined ? Promise.reject(new Error("unsupported")) : read()),
    enabled: read !== undefined,
    refetchOnWindowFocus: true,
  });
  const change = useMutation({
    mutationFn: async (action: HelperAction) => {
      switch (action) {
        case "enable":
          return transport.helperEnable?.();
        case "disable":
          return transport.helperDisable?.();
        case "open_login_items":
          await transport.helperOpenLoginItems?.();
          return undefined;
      }
    },
    onSettled: () => queries.invalidateQueries({ queryKey: HELPER }),
  });
  if (read === undefined || state.data === undefined) {
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
          <p className="mt-1">{h.notYet}</p>
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
