import type { AiSettingsView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
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
import { buttonClass, Disclosure, ErrorNotice } from "@/shared/ui";
import { StepHeading } from "./StepHeading";

/**
 * Whether to use AI review (ADR-0050). Nothing is sent before an answer, and
 * its two choices are equal. Turning AI review on stores a typed key first,
 * as the consent dialog does; a typed key stays in this step's state only
 * until it is stored, and is never shown.
 */
export function AiStep({
  settings,
  titleId,
  onDone,
}: {
  readonly settings: AiSettingsView;
  readonly titleId: string;
  readonly onDone: () => void;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
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
      onDone();
    },
  });
  const busy = choose.isPending;
  const field = (hint: string) => (
    <AiKeyField value={key} onChange={setKey} disabled={busy} hint={hint} />
  );
  return (
    <>
      <StepHeading id={titleId} title={m.ai.firstLaunch.title} intro={m.ai.firstLaunch.intro} />
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
    </>
  );
}
