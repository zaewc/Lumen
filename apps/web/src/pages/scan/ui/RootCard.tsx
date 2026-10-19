import type { RootView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";

/** One scanned root: its sizes and, above all, whether they are complete. */
export function RootCard({ root }: { readonly root: RootView }) {
  const { m } = useI18n();
  const reclaimable = formatBytes(root.reclaimable_bytes);
  return (
    <article className="rounded border border-neutral-200 p-4">
      <h3 className="font-mono text-sm">{root.root}</h3>
      <dl className="mt-2 grid grid-cols-2 gap-x-4 text-sm">
        <dt>{m.root.files}</dt>
        <dd>{m.root.fileCount(root.files)}</dd>
        <dt>{m.root.size}</dt>
        <dd>
          {m.root.sizeValue(formatBytes(root.logical_bytes), formatBytes(root.allocated_bytes))}
        </dd>
        <dt>{m.root.reclaimable}</dt>
        <dd>{root.reclaimable_exact ? reclaimable : m.root.atLeast(reclaimable)}</dd>
      </dl>
      {root.coverage_complete ? (
        <p className="mt-2 text-sm text-green-800">{m.root.complete}</p>
      ) : (
        <p className="mt-2 text-sm text-amber-800">
          {m.root.incomplete(root.denied, root.failed, root.not_scanned)}
        </p>
      )}
    </article>
  );
}
