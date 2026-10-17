import type { RootView } from "@lumen/api-client";
import { formatBytes } from "@/shared/lib";

/** One scanned root: its sizes and, above all, whether they are complete. */
export function RootCard({ root }: { readonly root: RootView }) {
  return (
    <article className="rounded border border-neutral-200 p-4">
      <h3 className="font-mono text-sm">{root.root}</h3>
      <dl className="mt-2 grid grid-cols-2 gap-x-4 text-sm">
        <dt>Files</dt>
        <dd>{root.files}</dd>
        <dt>Size</dt>
        <dd>
          {formatBytes(root.logical_bytes)} (allocated {formatBytes(root.allocated_bytes)})
        </dd>
        <dt>Reclaimable</dt>
        <dd>
          {formatBytes(root.reclaimable_bytes)}
          {root.reclaimable_exact ? "" : " at least"}
        </dd>
      </dl>
      {root.coverage_complete ? (
        <p className="mt-2 text-sm text-green-800">Everything below it was read.</p>
      ) : (
        <p className="mt-2 text-sm text-amber-800">
          Not everything could be read ({root.denied} denied, {root.failed} failed,{" "}
          {root.not_scanned} not scanned). The sizes are lower bounds, and nothing unread is treated
          as empty.
        </p>
      )}
    </article>
  );
}
