import type { AiSettingsView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useRef, useState } from "react";
import { isOn, useAiSettings } from "@/entities/ai-review";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { ErrorNotice, Group, GroupRow, IconTile, SparkleIcon, Switch } from "@/shared/ui";
import { AiConsentDialog } from "./AiConsentDialog";

/**
 * AI review (ADR-0037) as a setting: a switch that is off until the user
 * turns it on in the consent dialog, then on for every scan until turned
 * off here. Turning it off needs no dialog.
 */
export function AiReviewGroup() {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const settings = useAiSettings();
  const label = useId();
  const description = useId();
  const toggle = useRef<HTMLButtonElement>(null);
  const [asking, setAsking] = useState(false);
  const saved = (updated: AiSettingsView) => queries.setQueryData(QUERY_KEYS.aiSettings, updated);
  const turnOff = useMutation({
    mutationFn: () => transport.setAiReview(false),
    onSuccess: saved,
  });
  const view = settings.data;
  const service = view === undefined ? null : isOn(view);
  return (
    <div id="ai-review" className="scroll-mt-6 space-y-2">
      <Group>
        <GroupRow
          icon={
            <IconTile>
              <SparkleIcon />
            </IconTile>
          }
          title={m.ai.title}
          titleId={label}
          descriptionId={description}
          description={
            settings.isPending
              ? m.ai.checking
              : service === null
                ? m.ai.offIntro
                : m.ai.onIntro(service)
          }
          trailing={
            // Shown at once, so a slow answer (the system may be asking about
            // the keychain) never hides the setting; the switch waits for it.
            <div className="flex h-full items-center">
              <Switch
                ref={toggle}
                checked={service !== null}
                disabled={view === undefined || turnOff.isPending}
                labelledBy={label}
                describedBy={description}
                onChange={(next) => {
                  turnOff.reset();
                  if (next) {
                    setAsking(true);
                  } else {
                    turnOff.mutate();
                  }
                }}
              />
            </div>
          }
        />
      </Group>
      {settings.isError && <ErrorNotice message={m.ai.loadFailed} error={settings.error} />}
      {turnOff.isError && <ErrorNotice message={m.ai.changeFailed} error={turnOff.error} />}
      {view !== undefined && (
        <AiConsentDialog
          open={asking}
          settings={view}
          onDone={(updated) => {
            saved(updated);
            setAsking(false);
          }}
          onClose={() => setAsking(false)}
          returnFocus={toggle}
        />
      )}
    </div>
  );
}
