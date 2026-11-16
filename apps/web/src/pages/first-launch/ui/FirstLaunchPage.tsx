import type { AiSettingsView } from "@lumen/api-client";
import { useCallback, useId, useState } from "react";
import { useTransport } from "@/shared/api";
import { isMac, userAgent } from "@/shared/lib";
import { LogoMark } from "@/shared/ui";
import { setupSteps } from "../model/steps";
import { AiStep } from "./AiStep";
import { DiskStep } from "./DiskStep";
import { HelperStep } from "./HelperStep";

/**
 * The first-launch setup (ADR-0050, ADR-0067), shown while the user has not
 * chosen whether to use AI review: AI review, then the administrator helper,
 * then Full Disk Access, each where the host can offer it. Each choice
 * applies at once. It is a page, not a dialog: it takes no focus.
 */
export function FirstLaunchPage({
  settings,
  onDone,
}: {
  readonly settings: AiSettingsView;
  readonly onDone: () => void;
}) {
  const transport = useTransport();
  const title = useId();
  const [steps] = useState(() =>
    setupSteps({
      helper: transport.helperState !== undefined,
      disk: isMac(userAgent()) && transport.openFullDiskAccess !== undefined,
    }),
  );
  const [at, setAt] = useState(0);
  const next = useCallback(() => {
    if (at + 1 < steps.length) {
      setAt(at + 1);
    } else {
      onDone();
    }
  }, [at, steps.length, onDone]);
  const step = steps[at] ?? "ai";
  return (
    <main
      aria-labelledby={title}
      className="h-dvh overflow-y-auto overscroll-contain bg-canvas text-[14px] leading-relaxed text-ink"
    >
      <div className="mx-auto flex min-h-full w-full max-w-[560px] flex-col justify-center px-4 py-10">
        <LogoMark className="size-10" />
        {step === "ai" && <AiStep settings={settings} titleId={title} onDone={next} />}
        {step === "helper" && <HelperStep titleId={title} onDone={next} />}
        {step === "disk" && <DiskStep titleId={title} onDone={next} />}
      </div>
    </main>
  );
}
