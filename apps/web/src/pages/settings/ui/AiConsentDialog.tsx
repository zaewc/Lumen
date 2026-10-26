import type { AiSettingsView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, type RefObject, useId, useRef, useState } from "react";
import {
  canTurnOn,
  destination,
  needsKey,
  offersOwnKey,
  sourceOnConsent,
} from "@/entities/ai-review";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { isApiKey } from "@/shared/lib";
import { buttonClass, CheckIcon, Dialog, Disclosure, ErrorNotice } from "@/shared/ui";

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
  const description = useId();
  const dismiss = useRef<HTMLButtonElement>(null);
  const [key, setKey] = useState("");
  const asking = needsKey(settings);
  const turnOn = useMutation({
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
    if (canTurnOn(settings, key)) {
      turnOn.mutate();
    }
  };
  const busy = turnOn.isPending;
  const field = (hintText: string) => (
    <KeyField value={key} onChange={setKey} disabled={busy} hint={hintText} />
  );
  const points = [
    m.ai.consent.what,
    destination(settings) === "openrouter"
      ? m.ai.consent.whereOpenRouter
      : sourceOnConsent(settings, key) === "builtin"
        ? m.ai.consent.whereJebBuiltin
        : m.ai.consent.whereJeb,
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
        {asking && <div className="mt-5">{field(m.ai.consent.keyHint)}</div>}
        {offersOwnKey(settings) && (
          <Disclosure
            className="mt-5"
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

/** The Jeb key input: masked, never autofilled, checked as it is typed. */
function KeyField({
  value,
  onChange,
  disabled,
  hint,
}: {
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly disabled: boolean;
  readonly hint: string;
}) {
  const { m } = useI18n();
  const field = useId();
  const hintId = useId();
  const invalid = value.trim() !== "" && !isApiKey(value);
  return (
    <div className="space-y-1.5">
      <label htmlFor={field} className="block text-[13px] font-medium text-ink">
        {m.ai.consent.keyLabel}
      </label>
      <input
        id={field}
        type="password"
        autoComplete="off"
        spellCheck={false}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        aria-describedby={hintId}
        aria-invalid={invalid}
        disabled={disabled}
        className="h-9 w-full rounded-lg border border-line-strong bg-window px-3 font-mono text-[13px] text-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
      />
      <p id={hintId} className="text-[12px] text-ink-3">
        {invalid ? m.ai.consent.keyInvalid : hint}
      </p>
    </div>
  );
}
