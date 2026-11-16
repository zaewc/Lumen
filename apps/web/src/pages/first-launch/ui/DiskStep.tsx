import { useMutation } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass } from "@/shared/ui";
import { StepHeading } from "./StepHeading";

/**
 * Full Disk Access (ADR-0067): why Lumen asks and how to grant it. Lumen
 * cannot grant it or read whether it is granted, so the step opens System
 * Settings and finishes the setup either way.
 */
export function DiskStep({
  titleId,
  onDone,
}: {
  readonly titleId: string;
  readonly onDone: () => void;
}) {
  const { m } = useI18n();
  const p = m.settings.permissions;
  const s = m.setup;
  const open = useTransport().openFullDiskAccess;
  const opening = useMutation({ mutationFn: async () => open?.() });
  return (
    <>
      <StepHeading id={titleId} title={s.disk.title} intro={s.disk.intro} />
      <ol className="mt-5 list-decimal space-y-1 pl-5 text-ink-2 marker:text-ink-3">
        <li>{p.step2}</li>
        <li>{p.step3}</li>
        <li>{p.step4}</li>
      </ol>
      {opening.isError && (
        <p role="alert" className="mt-4 text-[13px] text-attention">
          {p.openFailed}
        </p>
      )}
      <div className="mt-8 grid gap-2 min-[480px]:grid-cols-2">
        <button type="button" onClick={onDone} className={buttonClass("secondary", "lg")}>
          {s.start}
        </button>
        {open !== undefined && (
          <button
            type="button"
            onClick={() => opening.mutate()}
            disabled={opening.isPending}
            className={buttonClass("primary", "lg")}
          >
            {p.open}
          </button>
        )}
      </div>
    </>
  );
}
