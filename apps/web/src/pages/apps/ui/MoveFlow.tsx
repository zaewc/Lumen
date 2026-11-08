import type { SoftwareItemView, UninstallOutcomeView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice, Notice } from "@/shared/ui";
import { title } from "../model/list";
import { failureText, useAppMove } from "../model/move";
import { MoveDialog, MoveReasons } from "./MoveDialog";

/**
 * Asks the host for a fresh preview of moving `item` to Quarantine (ADR-0062)
 * and confirms exactly that preview. The page sends only the app's key and the
 * preview's ID and hash; every fact shown comes from the host.
 */
export function MoveFlow({
  item,
  onClose,
}: {
  readonly item: SoftwareItemView;
  readonly onClose: () => void;
}) {
  const transport = useTransport();
  const ask = transport.uninstallPreview;
  const move = useAppMove();
  const preview = useQuery({
    queryKey: ["software", "uninstall-preview", item.key],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask(item.key)),
    retry: false,
    gcTime: 0,
    staleTime: 0,
    refetchOnWindowFocus: false,
  });
  return (
    <MoveDialog
      name={title(item)}
      preview={preview.data ?? null}
      error={preview.isError ? preview.error : null}
      loading={preview.isFetching}
      onClose={onClose}
      onConfirm={(ready) => {
        onClose();
        void move.run(ready);
      }}
    />
  );
}

function OutcomeText({
  outcome,
  name,
}: {
  readonly outcome: UninstallOutcomeView;
  readonly name: string;
}) {
  const { m } = useI18n();
  const a = m.apps.move;
  const result = outcome.result;
  switch (result.kind) {
    case "quarantined":
      return (
        <p>
          {a.moved(name)} {a.noSpace}
        </p>
      );
    case "refused":
      return (
        <>
          <p className="font-medium">{a.refused}</p>
          <MoveReasons reasons={result.reasons} />
        </>
      );
    case "changed":
      return <p>{a.changed}</p>;
    case "failed":
      return (
        <p>
          {a.failedMove} {failureText(m, result.reason)}. {a.locations[result.left]}
        </p>
      );
  }
}

/** The last move's result, kept for the session so leaving the page loses nothing. */
export function MoveResult({ name }: { readonly name: (key: string) => string | null }) {
  const { m } = useI18n();
  const a = m.apps.move;
  const move = useAppMove();
  if (move.status === "idle") return null;
  const label =
    move.variables === null
      ? ""
      : (name(move.variables.app.key) ?? move.variables.app.name ?? move.variables.app.bundle_id);
  if (move.status === "pending")
    return (
      <p role="status" className="text-[13px] text-ink-2">
        {a.moving(label)}
      </p>
    );
  if (move.status === "error") return <ErrorNotice message={a.failed} error={move.error} />;
  const outcome = move.data;
  if (outcome === null) return null;
  const moved = outcome.result.kind === "quarantined";
  return (
    <Notice tone={moved ? "success" : "warning"}>
      <div className="space-y-2">
        <OutcomeText outcome={outcome} name={label} />
        {outcome.warnings.map((warning) => (
          <p key={warning}>{warning}</p>
        ))}
        <div className="flex flex-wrap gap-2">
          {(moved || outcome.result.kind === "failed") && (
            <Link to="/quarantine" className={buttonClass("secondary", "sm")}>
              {m.done.openQuarantine}
            </Link>
          )}
          <button type="button" onClick={move.reset} className={buttonClass("secondary", "sm")}>
            {m.common.close}
          </button>
        </div>
      </div>
    </Notice>
  );
}
