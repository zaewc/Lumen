import type { AiSettingsView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, type RefObject, useId, useRef, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { isApiKey } from "@/shared/lib";
import { buttonClass, CheckIcon, Dialog, ErrorNotice } from "@/shared/ui";
import { canTurnOn, destination, needsKey } from "../model/ai";

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
  const cancel = useRef<HTMLButtonElement>(null);
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
      initialFocus={cancel}
      returnFocus={returnFocus}
    >
      <form onSubmit={submit}>
        <h2 id={title} className="text-lg font-semibold tracking-tight">
          {m.ai.consent.title}
        </h2>
        <ul
          id={description}
          className="mt-4 space-y-3 text-sm text-neutral-700 dark:text-neutral-300"
        >
          {points.map((point) => (
            <li key={point} className="flex gap-3">
              <CheckIcon className="text-indigo-600 dark:text-indigo-400" />
              <span>{point}</span>
            </li>
          ))}
        </ul>
        {asking && (
          <div className="mt-5 space-y-2">
            <label
              htmlFor={field}
              className="block text-sm font-medium text-neutral-700 dark:text-neutral-300"
            >
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
              className="w-full rounded-xl border border-neutral-300 bg-white p-3 font-mono text-sm focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-indigo-600 dark:border-neutral-600 dark:bg-neutral-950 dark:focus-visible:outline-indigo-400"
            />
            <p id={hint} className="text-xs text-neutral-500 dark:text-neutral-400">
              {invalid ? m.ai.consent.keyInvalid : m.ai.consent.keyHint}
            </p>
          </div>
        )}
        {turnOn.isError && (
          <div className="mt-4">
            <ErrorNotice message={m.ai.changeFailed} error={turnOn.error} />
          </div>
        )}
        <div className="mt-6 flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <button
            ref={cancel}
            type="button"
            onClick={close}
            disabled={busy}
            className={buttonClass("secondary")}
          >
            {m.common.cancel}
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
