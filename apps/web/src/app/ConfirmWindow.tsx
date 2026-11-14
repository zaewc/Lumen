import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { confirmHost, type HostAsk, type HostAskItem } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { Bytes, buttonClass, CheckIcon } from "@/shared/ui";

/** One item: its name, the folder it is in, and what it frees when known. */
function Row({ item }: { readonly item: HostAskItem }) {
  return (
    <li className="flex items-baseline gap-3 py-2">
      {/* Whole, never cut off: this is what the person confirms. */}
      <span className="min-w-0 flex-1 select-text">
        <span className="block break-words text-ink">{item.name}</span>
        <span className="block break-all text-[12px] text-ink-3">{item.folder}</span>
      </span>
      {item.bytes !== null && (
        <span className="shrink-0 tabular-nums text-ink">
          <Bytes value={item.bytes} />
        </span>
      )}
    </li>
  );
}

function Rows({ items, more }: { readonly items: readonly HostAskItem[]; readonly more: number }) {
  const { m } = useI18n();
  return (
    <ul className="mt-2 divide-y divide-line border-y border-line text-[14px]">
      {items.map((item) => (
        <Row key={item.path} item={item} />
      ))}
      {more > 0 && <li className="py-2 text-[13px] text-ink-2">{m.hostConfirm.more(more)}</li>}
    </ul>
  );
}

function Group({
  title,
  note,
  children,
}: {
  readonly title: string;
  readonly note?: string;
  readonly children: ReactNode;
}) {
  return (
    <section className="mt-5">
      <h2 className="text-section text-ink">{title}</h2>
      {note !== undefined && <p className="mt-1 text-[13px] leading-relaxed text-ink-2">{note}</p>}
      {children}
    </section>
  );
}

function Notes({ notes }: { readonly notes: readonly string[] }) {
  return (
    <ul className="mt-3 space-y-1.5 text-[14px] text-ink-2">
      {notes.map((note) => (
        <li key={note} className="flex gap-2">
          <CheckIcon className="mt-0.5 size-4 shrink-0 text-ink-3" />
          <span>{note}</span>
        </li>
      ))}
    </ul>
  );
}

/** The question's title, body and action, in the page's language. */
function useQuestion(ask: HostAsk): {
  readonly title: string;
  readonly body: ReactNode;
  readonly action: string;
  readonly danger: boolean;
} {
  const { m } = useI18n();
  const h = m.hostConfirm;
  switch (ask.kind) {
    case "move":
      return {
        title: m.confirm.title(ask.count, formatBytes(ask.total_bytes)),
        action: m.confirm.move,
        danger: false,
        body: (
          <>
            <Notes notes={[m.confirm.checked, m.confirm.restorable]} />
            {ask.proposed.items.length > 0 && (
              <Group title={h.proposed}>
                <Rows items={ask.proposed.items} more={ask.proposed.more} />
              </Group>
            )}
            {ask.chosen.length > 0 && (
              <Group title={h.chosen} note={h.chosenNote}>
                <Rows items={ask.chosen} more={0} />
              </Group>
            )}
          </>
        ),
      };
    case "uninstall":
      return {
        title: h.uninstallTitle,
        action: m.confirm.move,
        danger: false,
        body: (
          <>
            <Notes notes={[h.uninstallNote]} />
            <Rows items={ask.items} more={0} />
          </>
        ),
      };
    case "finalize":
      return {
        title: h.finalizeTitle,
        action: h.finalizeButton,
        danger: true,
        body: (
          <>
            <Rows items={[ask.item]} more={0} />
            {ask.chosen && <p className="mt-3 text-[13px] text-ink-2">{h.finalizeChosen}</p>}
            <p className="mt-3 text-[14px] font-medium text-ink">{h.finalizeNote}</p>
          </>
        ),
      };
    case "tool":
      return {
        title: h.toolTitle(ask.tool),
        action: h.toolButton,
        danger: true,
        body: (
          <>
            <pre className="mt-3 whitespace-pre-wrap break-all rounded bg-fill px-3 py-2 font-mono text-[13px] text-ink select-text">
              {ask.command}
            </pre>
            <Rows items={ask.targets.items} more={ask.targets.more} />
            <p className="mt-3 text-[14px] font-medium text-ink">{h.toolNote}</p>
          </>
        ),
      };
  }
}

function Question({
  ask,
  onAnswer,
}: {
  readonly ask: HostAsk;
  readonly onAnswer: (yes: boolean) => void;
}) {
  const { m } = useI18n();
  const cancel = useRef<HTMLButtonElement>(null);
  const { title, body, action, danger } = useQuestion(ask);
  // Enter on a stray key press cancels: the action needs a deliberate click.
  useEffect(() => cancel.current?.focus(), []);
  return (
    <main className="flex h-screen flex-col bg-raised text-ink">
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-4">
        <h1 className="text-[17px] font-semibold text-ink">{title}</h1>
        {body}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-4">
        <button
          ref={cancel}
          type="button"
          onClick={() => onAnswer(false)}
          className={buttonClass("secondary")}
        >
          {m.common.close}
        </button>
        <button
          type="button"
          onClick={() => onAnswer(true)}
          className={buttonClass(danger ? "danger" : "primary")}
        >
          {action}
        </button>
      </div>
    </main>
  );
}

/**
 * The desktop host's confirmation window (FS-1): shows the question the host
 * built and sends the answer. Escape, Cancel or closing the window refuses.
 */
export function ConfirmWindow({
  host = confirmHost,
}: {
  /** The host's calls; a sample in a plain browser (development). */
  readonly host?: typeof confirmHost;
}) {
  const { m } = useI18n();
  const [ask, setAsk] = useState<HostAsk | null>(null);
  const [failed, setFailed] = useState(false);
  const answered = useRef(false);
  const answer = useCallback(
    (yes: boolean) => {
      if (answered.current) {
        return;
      }
      answered.current = true;
      // The host closes the window either way; a failed answer is a refusal.
      host.answer(yes).catch(() => undefined);
    },
    [host],
  );
  useEffect(() => {
    host.request().then(setAsk, () => setFailed(true));
  }, [host]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        answer(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [answer]);
  if (failed) {
    return (
      <main className="flex h-screen flex-col justify-between bg-raised p-6 text-ink">
        <p className="text-[14px]">{m.hostConfirm.unavailable}</p>
        <div className="flex justify-end">
          <button type="button" onClick={() => answer(false)} className={buttonClass("secondary")}>
            {m.common.close}
          </button>
        </div>
      </main>
    );
  }
  return ask === null ? (
    <main className="h-screen bg-raised" />
  ) : (
    <Question ask={ask} onAnswer={answer} />
  );
}
