import type { AiSettingsView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, type RefObject, useId, useRef, useState } from "react";
import {
  AI_WRITE_KEY,
  AiKeyField,
  AiReviewFacts,
  canTurnOn,
  needsKey,
  offersOwnKey,
  useAiWriteState,
} from "@/entities/ai-review";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Dialog, Disclosure, ErrorNotice } from "@/shared/ui";

/**
 * Asks before turning AI review on (ADR-0037): what is sent, where, and that
 * it can only make Lumen more careful. Without any key it also takes a Jeb
 * key, stored before AI review is turned on. With Lumen's built-in key
 * (ADR-0042) it asks for none, but offers to store the user's own behind a
 * disclosure. A typed key stays in this dialog's state only until it is
 * stored, and is never shown.
 */
export function AiConsentDialog({
  open,
  settings,
  onDone,
  onClose,
  returnFocus,
}: {
  readonly open: boolean;
  readonly settings: AiSettingsView;
  readonly onDone: (settings: AiSettingsView) => void;
  readonly onClose: () => void;
  readonly returnFocus: RefObject<HTMLElement | null>;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const title = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const [key, setKey] = useState("");
  const asking = needsKey(settings);
  const writing = useAiWriteState()?.status === "pending";
  const turnOn = useMutation({
    mutationKey: AI_WRITE_KEY,
    mutationFn: async () => {
      if (key.trim() !== "") {
        await transport.setAiKey(key.trim());
      }
      return transport.setAiReview(true);
    },
    onSuccess: (updated) => {
      setKey("");
      onDone(updated);
    },
  });
  const close = () => {
    setKey("");
    turnOn.reset();
    onClose();
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!writing && canTurnOn(settings, key)) {
      turnOn.mutate();
    }
  };
  const busy = writing;
  const field = (hintText: string) => (
    <AiKeyField value={key} onChange={setKey} disabled={busy} hint={hintText} />
  );
  return (
    <Dialog
      open={open}
      onClose={close}
      labelledBy={title}
      dismissible={!busy}
      initialFocus={heading}
      returnFocus={returnFocus}
    >
      <form onSubmit={submit} className="flex max-h-[calc(100dvh-5rem)] flex-col">
        <h2
          ref={heading}
          id={title}
          tabIndex={-1}
          className="shrink-0 text-[22px] font-semibold tracking-[-0.03em] text-ink outline-none"
        >
          {m.ai.consent.title}
        </h2>
        <div className="-mx-1 mt-4 min-h-0 overflow-x-hidden overflow-y-auto px-1">
          <AiReviewFacts settings={settings} keyText={key} />
          {asking && <div className="mt-5">{field(m.ai.consent.keyHint)}</div>}
          {offersOwnKey(settings) && (
            <Disclosure
              className="mx-2 mt-5"
              summary={<span className="text-[13px] text-ink-2">{m.ai.consent.ownKey}</span>}
            >
              {field(m.ai.consent.ownKeyHint)}
            </Disclosure>
          )}
          {turnOn.isError && (
            <div className="mt-4">
              <ErrorNotice message={m.ai.changeFailed} error={turnOn.error} />
            </div>
          )}
        </div>
        <div className="mt-6 flex shrink-0 flex-col-reverse gap-2 min-[480px]:flex-row min-[480px]:justify-end">
          <button
            type="button"
            onClick={close}
            disabled={busy}
            className={buttonClass("secondary")}
          >
            {m.common.close}
          </button>
          <button
            type="submit"
            disabled={busy || !canTurnOn(settings, key)}
            className={buttonClass("primary")}
          >
            {busy ? m.ai.consent.turningOn : m.ai.consent.confirm}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
