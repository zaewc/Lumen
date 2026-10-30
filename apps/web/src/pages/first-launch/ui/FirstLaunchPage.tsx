import type { AiSettingsView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import {
  AI_WRITE_KEY,
  AiKeyField,
  AiReviewFacts,
  canTurnOn,
  needsKey,
  offersOwnKey,
} from "@/entities/ai-review";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Disclosure, ErrorNotice, LogoMark } from "@/shared/ui";

/**
 * The first screen while the user has not chosen whether to use AI review
 * (ADR-0050). Nothing is sent before an answer. It is a page, not a dialog:
 * it takes no focus, and its two choices are equal. Turning AI review on
 * stores a typed key first, as the consent dialog does; a typed key stays in
 * this page's state only until it is stored, and is never shown.
 */
export function FirstLaunchPage({ settings }: { readonly settings: AiSettingsView }) {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const title = useId();
  const [key, setKey] = useState("");
  const choose = useMutation({
    mutationKey: AI_WRITE_KEY,
    mutationFn: async (enabled: boolean) => {
      if (enabled && key.trim() !== "") {
        await transport.setAiKey(key.trim());
      }
      return transport.setAiReview(enabled);
    },
    onSuccess: (updated) => {
      setKey("");
      queries.setQueryData(QUERY_KEYS.aiSettings, updated);
    },
  });
  const busy = choose.isPending;
  const field = (hint: string) => (
    <AiKeyField value={key} onChange={setKey} disabled={busy} hint={hint} />
  );
  return (
    <main
      aria-labelledby={title}
      className="h-dvh overflow-y-auto overscroll-contain bg-window text-[14px] leading-relaxed text-ink"
    >
      <div className="mx-auto flex min-h-full w-full max-w-[560px] flex-col justify-center px-4 py-10">
        <LogoMark className="size-10" />
        <h1 id={title} className="mt-5 text-[24px] font-semibold tracking-[-0.03em] text-ink">
          {m.ai.firstLaunch.title}
        </h1>
        <p className="mt-2 text-ink-2">{m.ai.firstLaunch.intro}</p>
        <div className="mt-6">
          <AiReviewFacts settings={settings} keyText={key} />
        </div>
        {needsKey(settings) && <div className="mt-5">{field(m.ai.consent.keyHint)}</div>}
        {offersOwnKey(settings) && (
          <Disclosure
            className="mx-2 mt-5"
            summary={<span className="text-[13px] text-ink-2">{m.ai.consent.ownKey}</span>}
          >
            {field(m.ai.consent.ownKeyHint)}
          </Disclosure>
        )}
        {choose.isError && (
          <div className="mt-4">
            <ErrorNotice message={m.ai.changeFailed} error={choose.error} />
          </div>
        )}
        <div className="mt-8 grid gap-2 min-[480px]:grid-cols-2">
          <button
            type="button"
            onClick={() => choose.mutate(false)}
            disabled={busy}
            className={buttonClass("secondary", "lg")}
          >
            {m.ai.firstLaunch.off}
          </button>
          <button
            type="button"
            onClick={() => choose.mutate(true)}
            disabled={busy || !canTurnOn(settings, key)}
            className={buttonClass("secondary", "lg")}
          >
            {m.ai.firstLaunch.on}
          </button>
        </div>
      </div>
    </main>
  );
}
