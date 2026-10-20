import type { RootView, ScanView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { Card, Disclosure } from "@/shared/ui";

/** One scanned folder: its sizes and whether all of it was read. */
function RootDetails({ root }: { readonly root: RootView }) {
  const { m } = useI18n();
  const reclaimable = formatBytes(root.reclaimable_bytes);
  return (
    <li className="rounded-xl bg-neutral-50 p-3 text-sm dark:bg-neutral-800/60">
      <p className="font-mono text-xs break-all text-neutral-700 dark:text-neutral-300">
        {root.root}
      </p>
      <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        <dt className="text-neutral-500 dark:text-neutral-400">{m.details.files}</dt>
        <dd className="tabular-nums">{m.details.fileCount(root.files)}</dd>
        <dt className="text-neutral-500 dark:text-neutral-400">{m.details.size}</dt>
        <dd className="tabular-nums">
          {m.details.sizeValue(formatBytes(root.logical_bytes), formatBytes(root.allocated_bytes))}
        </dd>
        <dt className="text-neutral-500 dark:text-neutral-400">{m.details.reclaimable}</dt>
        <dd className="tabular-nums">
          {root.reclaimable_exact ? reclaimable : m.details.atLeast(reclaimable)}
        </dd>
      </dl>
      <p
        className={`mt-2 ${root.coverage_complete ? "text-neutral-600 dark:text-neutral-400" : "text-amber-800 dark:text-amber-300"}`}
      >
        {root.coverage_complete
          ? m.details.complete
          : m.details.incomplete(root.denied, root.failed, root.not_scanned)}
      </p>
    </li>
  );
}

/** Per-folder numbers, for those who want them; closed by default. */
export function ScanDetails({ scan }: { readonly scan: ScanView }) {
  const { m } = useI18n();
  return (
    <Card className="py-4 sm:py-4">
      <Disclosure
        summary={
          <span className="font-medium text-neutral-700 dark:text-neutral-300">
            {m.details.title}
          </span>
        }
      >
        <p className="text-sm text-neutral-600 dark:text-neutral-400">
          {m.details.entries(scan.entries)}
        </p>
        <ul className="mt-3 space-y-2">
          {scan.roots.map((root) => (
            <RootDetails key={root.root} root={root} />
          ))}
        </ul>
      </Disclosure>
    </Card>
  );
}
