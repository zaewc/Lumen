import { useQuery } from "@tanstack/react-query";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { readPermille } from "../model/resolving";

/**
 * The map's frame before there is a map (docs/design/visual-language.md,
 * understanding resolving): one dashed "Not read yet" region before a scan;
 * while it reads, a hatch filling from the left as its size on disk grows
 * against the disk's used space. No regions until there are decisions.
 */
export function MapFrame({
  read,
}: {
  /** Bytes read so far; `null` before a scan. */ readonly read: string | null;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const disk = useQuery({
    queryKey: QUERY_KEYS.disk,
    queryFn: () => transport.disk(),
    retry: false,
    enabled: read !== null,
  });
  const frame = "map-frame relative h-[200px] min-[640px]:h-[280px]";
  if (read === null) {
    return (
      <div aria-hidden="true" className={frame}>
        <span className="topo-label">{m.results.map.notReadYet}</span>
      </div>
    );
  }
  const permille = readPermille(disk.data ?? null, read);
  const label = m.scanning.readSoFar(formatBytes(read));
  return (
    <div
      role="progressbar"
      aria-label={m.scanning.progress}
      aria-valuetext={label}
      className={frame}
    >
      {permille !== null && (
        <div
          className="map-read absolute inset-y-0 left-0"
          style={{ width: `${permille / 10}%` }}
        />
      )}
      <span className="topo-label relative w-fit bg-canvas tabular-nums">{label}</span>
    </div>
  );
}
