import type { HomebrewOutcomeView, HomebrewPreviewView, SoftwareItemView } from "@lumen/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, CONFIRM_ROW, ErrorNotice, Notice } from "@/shared/ui";

type Ready = Extract<HomebrewPreviewView, { kind: "ready" }>;

/** Whether `item` is a Homebrew formula or cask. */
export function isHomebrew(item: SoftwareItemView): boolean {
  return item.origin === "homebrew" || item.origin === "homebrew_cask";
}

function Outcome({ outcome }: { readonly outcome: HomebrewOutcomeView }) {
  const { m } = useI18n();
  const h = m.apps.homebrew;
  const name = outcome.package?.name ?? "";
  const { result, ran } = outcome;
  const message = (() => {
    switch (result.kind) {
      case "removed":
        return <Notice tone="success">{h.removed(name)}</Notice>;
      case "still_listed":
        return <Notice tone="error">{h.stillListed}</Notice>;
      case "unverified":
        return <Notice tone="warning">{h.unverified}</Notice>;
      case "changed":
        return <Notice tone="warning">{h.changed}</Notice>;
      case "refused":
        return (
          <Notice tone="warning">
            <p>{h.refused}</p>
            <ul className="list-disc pl-4">
              {result.reasons.map((reason) => (
                <li key={reason}>{h.reasons[reason]}</li>
              ))}
            </ul>
          </Notice>
        );
    }
  })();
  return (
    <div className="space-y-2">
      {message}
      {ran !== null && (
        <div className="space-y-1 text-[12px] text-ink-2">
          {ran.timed_out && <p>{h.stopped}</p>}
          {!ran.success && ran.exit_code !== null && <p>{h.exit(ran.exit_code)}</p>}
          {ran.output !== "" && (
            <>
              <p className="text-[11px] font-medium text-ink-3">{h.output}</p>
              <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-fill px-3 py-2 font-mono text-[11px] text-ink select-text">
                {ran.output}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Removing a Homebrew package through Homebrew, inside its details
 * (ADR-0060, roadmap 16.7). The host is asked for a fresh preview; the action
 * is offered only when it is ready, otherwise the reasons Lumen keeps the
 * package are shown. The page sends the list key, then the preview's ID and
 * hash, never a command; the host shows the exact command again in its own
 * window before anything runs. It cannot be undone.
 */
export function HomebrewRemoval({ item }: { readonly item: SoftwareItemView }) {
  const { m } = useI18n();
  const h = m.apps.homebrew;
  const transport = useTransport();
  const ask = transport.homebrewPreview;
  const remove = transport.homebrewRemove;
  const client = useQueryClient();
  const [reviewing, setReviewing] = useState(false);
  const preview = useQuery({
    queryKey: ["software", "homebrew-preview", item.key],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask(item.key)),
    enabled: ask !== undefined && remove !== undefined,
    retry: false,
    gcTime: 0,
    staleTime: 0,
    refetchOnWindowFocus: false,
  });
  const run = useMutation({
    mutationFn: (ready: Ready) =>
      remove === undefined
        ? Promise.reject(new Error("unsupported"))
        : remove(ready.preview, ready.hash),
    // The list again, so the row reflects what Homebrew now lists; only the
    // list itself, never another AI review of it.
    onSettled: () => client.invalidateQueries({ queryKey: ["software"], exact: true }),
  });
  if (run.data !== undefined) return <Outcome outcome={run.data} />;
  if (run.isError) return <ErrorNotice message={h.failed} error={run.error} />;
  if (run.isPending) {
    return <p className="text-[13px] text-ink-2">{h.removing(item.id)}</p>;
  }
  if (preview.isFetching) return <p className="text-[13px] text-ink-2">{h.checking}</p>;
  if (preview.isError) return <ErrorNotice message={h.failed} error={preview.error} />;
  const data = preview.data;
  if (data === undefined) return null;
  switch (data.kind) {
    case "not_found":
    case "unsupported":
      return null;
    case "refused":
      return (
        <div>
          <p className="text-[11px] font-medium text-ink-3">{h.refused}</p>
          <ul className="text-[13px] text-ink-2">
            {data.reasons.map((reason) => (
              <li key={reason}>{h.reasons[reason]}</li>
            ))}
          </ul>
        </div>
      );
    case "ready":
      if (!reviewing) {
        return (
          <div className="flex justify-end">
            <button
              type="button"
              onClick={() => setReviewing(true)}
              className={buttonClass("secondary")}
            >
              {h.action}
            </button>
          </div>
        );
      }
      return (
        <div className="space-y-2">
          <p className="text-[13px] font-medium text-ink">{h.title(data.package.name)}</p>
          <p className="text-[12px] text-ink-2">{h.command}</p>
          <pre className="whitespace-pre-wrap break-all rounded bg-fill px-3 py-2 font-mono text-[12px] text-ink select-text">
            {data.command.join(" ")}
          </pre>
          {data.package.apps.length > 0 && (
            <div>
              <p className="text-[11px] font-medium text-ink-3">{h.apps}</p>
              <ul className="font-mono text-[11px] break-all text-ink-2">
                {data.package.apps.map((app) => (
                  <li key={app}>{app}</li>
                ))}
              </ul>
            </div>
          )}
          <ul className="space-y-1 text-[13px] text-ink-2">
            {data.package.notes.map((note) => (
              <li key={note}>{h.notes[note]}</li>
            ))}
          </ul>
          <div className={`${CONFIRM_ROW} justify-end`}>
            <button
              type="button"
              onClick={() => run.mutate(data)}
              className={buttonClass("danger")}
            >
              {h.confirm}
            </button>
          </div>
        </div>
      );
  }
}
