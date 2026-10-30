import type { AiSettingsView } from "@lumen/api-client";
import { useId } from "react";
import { useI18n } from "@/shared/i18n";
import { isApiKey } from "@/shared/lib";
import { destination, sourceOnConsent } from "../model/ai";

/**
 * What turning AI review on means (ADR-0037, ADR-0050): what is sent, where
 * and with whose key, that it can only make Lumen more careful, and that the
 * choice can be changed. `keyText` is a key the user is typing, which decides
 * whose key the request would carry.
 */
export function AiReviewFacts({
  settings,
  keyText,
}: {
  readonly settings: AiSettingsView;
  readonly keyText: string;
}) {
  const { m } = useI18n();
  const points = [
    m.ai.consent.what,
    destination(settings) === "openrouter"
      ? m.ai.consent.whereOpenRouter
      : sourceOnConsent(settings, keyText) === "builtin"
        ? m.ai.consent.whereJebBuiltin
        : m.ai.consent.whereJeb,
    m.ai.consent.cautious,
    m.ai.consent.revocable,
  ];
  return (
    <ol className="divide-y divide-line text-[14px] leading-relaxed text-ink-2">
      {points.map((point, index) => (
        <li key={point} className="grid grid-cols-[1.25rem_minmax(0,1fr)] gap-3 py-3 first:pt-0">
          <span aria-hidden="true" className="pt-0.5 font-mono text-[11px] text-ink-3">
            {String(index + 1).padStart(2, "0")}
          </span>
          <span>{point}</span>
        </li>
      ))}
    </ol>
  );
}

/** The Jeb key input: masked, never autofilled, checked as it is typed. */
export function AiKeyField({
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
