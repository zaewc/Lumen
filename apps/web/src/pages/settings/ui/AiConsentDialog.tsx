import type { AiSettingsView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, type RefObject, useId, useRef, useState } from "react";
import { canTurnOn, destination, needsKey } from "@/entities/ai-review";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { isApiKey } from "@/shared/lib";
import { buttonClass, CheckIcon, Dialog, ErrorNotice } from "@/shared/ui";

/**
 * Asks before turning AI review on (ADR-0037): what is sent, where, and that
 * it can only make Lumen more careful. Without a stored key it also takes a
 * Jeb key, stored before AI review is turned on. The key stays in this
 * dialog's state only until it is stored, and is never shown.
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
  const description = useId();
  const field = useId();
  const hint = useId();
  const dismiss = useRef<HTMLButtonElement>(null);
  const [key, setKey] = useState("");
  const asking = needsKey(settings);
  const turnOn = useMutation({
    mutationFn: async () => {
      if (asking) {
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
    if (canTurnOn(settings, key)) {
      turnOn.mutate();
    }
  };
  const busy = turnOn.isPending;
  const invalid = asking && key.trim() !== "" && !isApiKey(key);
  const points = [
    m.ai.consent.what,
    destination(settings) === "jeb" ? m.ai.consent.whereJeb : m.ai.consent.whereOpenRouter,
    m.ai.consent.cautious,
    m.ai.consent.revocable,
  ];
  return (
    <Dialog
      open={open}
      onClose={close}
      labelledBy={title}
      describedBy={description}
      dismissible={!busy}
      initialFocus={dismiss}
      returnFocus={returnFocus}
    >
      <form onSubmit={submit}>
        <h2 id={title} className="text-[15px] font-semibold text-ink">
          {m.ai.consent.title}
        </h2>
        <ul id={description} className="mt-3 space-y-2.5 text-[13px] leading-snug text-ink-2">
          {points.map((point) => (
            <li key={point} className="flex gap-2.5">
              <CheckIcon className="size-[18px] text-accent-ink" />
              <span>{point}</span>
            </li>
          ))}
        </ul>
        {asking && (
          <div className="mt-5 space-y-1.5">
            <label htmlFor={field} className="block text-[13px] font-medium text-ink">
              {m.ai.consent.keyLabel}
            </label>
            <input
              id={field}
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={key}
              onChange={(event) => setKey(event.target.value)}
              aria-describedby={hint}
              aria-invalid={invalid}
              disabled={busy}
              className="h-9 w-full rounded-lg border border-line-strong bg-window px-3 font-mono text-[13px] text-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            />
            <p id={hint} className="text-[12px] text-ink-3">
              {invalid ? m.ai.consent.keyInvalid : m.ai.consent.keyHint}
            </p>
          </div>
        )}
        {turnOn.isError && (
          <div className="mt-4">
            <ErrorNotice message={m.ai.changeFailed} error={turnOn.error} />
          </div>
        )}
        <div className="mt-6 flex flex-col-reverse gap-2 min-[480px]:flex-row min-[480px]:justify-end">
          <button
            ref={dismiss}
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
