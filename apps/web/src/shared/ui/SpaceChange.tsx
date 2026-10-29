import type { SpaceChangeView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { formatBytes, summarizeSpaceChanges } from "@/shared/lib";

/** Observed disk changes stay distinct from estimates and cleanup success. */
export function SpaceChangeResult({
  changes,
  expected = null,
}: {
  readonly changes: readonly SpaceChangeView[] | null;
  readonly expected?: string | null;
}) {
  const { m } = useI18n();
  const text = m.spaceChange;
  const summary = summarizeSpaceChanges(changes);
  const headline =
    summary === null
      ? text.unknown
      : summary.delta > 0n
        ? text.increased(formatBytes(summary.delta.toString()))
        : summary.delta < 0n
          ? text.decreased(formatBytes((-summary.delta).toString()))
          : text.unchanged;
  return (
    <div className="mt-2 space-y-1 text-[13px] text-ink-2">
      {expected !== null && <p>{text.estimate(formatBytes(expected))}</p>}
      <p className="font-medium tabular-nums text-ink">{headline}</p>
      {summary !== null && (
        <details className="text-[12px]">
          <summary className="cursor-pointer">{text.details}</summary>
          {summary.readings.map((reading, index) => (
            <p key={reading.volume} className="mt-1 tabular-nums">
              {text.disk(index + 1)}:{" "}
              {text.readings(
                formatBytes(reading.before_available_bytes),
                formatBytes(reading.after_available_bytes),
              )}
            </p>
          ))}
        </details>
      )}
      <p className="text-[12px]">{text.note}</p>
    </div>
  );
}
