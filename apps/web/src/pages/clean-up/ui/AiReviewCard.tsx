import type { AiSettingsView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Card, ErrorNotice } from "@/shared/ui";
import { AI_SETTINGS, useAiSettings } from "../api/aiSettings";
import { serviceName } from "../model/ai";
import { AiConsentDialog } from "./AiConsentDialog";

/**
 * AI review on the start page (ADR-0037): off until the user turns it on in
 * the consent dialog, then on for every scan until turned off here.
 */
export function AiReviewCard() {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const settings = useAiSettings();
  const button = useRef<HTMLButtonElement>(null);
  const [asking, setAsking] = useState(false);
  const saved = (updated: AiSettingsView) => queries.setQueryData(AI_SETTINGS, updated);
  const turnOff = useMutation({
    mutationFn: () => transport.setAiReview(false),
    onSuccess: saved,
  });
  if (settings.isPending) {
    return null;
  }
  if (settings.isError) {
    return <ErrorNotice message={m.ai.loadFailed} error={settings.error} />;
  }
  const view = settings.data;
  const service = serviceName(view.key);
  const on = view.enabled && service !== null;
  return (
    <Card>
      <div className="space-y-3">
        <h2 className="font-semibold tracking-tight">{on ? m.ai.on(service) : m.ai.title}</h2>
        <p className="text-sm text-neutral-600 dark:text-neutral-400">
          {on ? m.ai.onIntro : m.ai.offIntro}
        </p>
        {turnOff.isError && <ErrorNotice message={m.ai.changeFailed} error={turnOff.error} />}
        {on ? (
          <button
            ref={button}
            type="button"
            onClick={() => turnOff.mutate()}
            disabled={turnOff.isPending}
            className={buttonClass("secondary", "sm")}
          >
            {turnOff.isPending ? m.ai.turningOff : m.ai.turnOff}
          </button>
        ) : (
          <button
            ref={button}
            type="button"
            onClick={() => {
              turnOff.reset();
              setAsking(true);
            }}
            className={buttonClass("secondary", "sm")}
          >
            {m.ai.turnOn}
          </button>
        )}
      </div>
      <AiConsentDialog
        open={asking}
        settings={view}
        onDone={(updated) => {
          saved(updated);
          setAsking(false);
        }}
        onClose={() => setAsking(false)}
        returnFocus={button}
      />
    </Card>
  );
}

/** One line on the results page: whether AI review took part. */
export function AiReviewStatus() {
  const { m } = useI18n();
  const settings = useAiSettings();
  if (!settings.isSuccess) {
    return null;
  }
  const service = serviceName(settings.data.key);
  return (
    <p className="text-sm text-neutral-500 dark:text-neutral-400">
      {settings.data.enabled && service !== null ? m.ai.statusOn(service) : m.ai.statusOff}
    </p>
  );
}
