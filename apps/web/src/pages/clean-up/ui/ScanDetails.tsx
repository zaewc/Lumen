import type { RootView, ScanView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, Disclosure } from "@/shared/ui";

/** One scanned folder: its sizes and whether all of it was read. */
function RootDetails({ root }: { readonly root: RootView }) {
  const { m } = useI18n();
  const reclaimable = formatBytes(root.reclaimable_bytes);
  return (
    <li className="py-2.5 first:pt-1 last:pb-1">
      <p className="font-mono text-[12px] break-all text-ink select-text">{root.root}</p>
      <dl className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5">
        <dt className="text-ink-3">{m.details.files}</dt>
        <dd className="tabular-nums text-ink-2">{m.details.fileCount(root.files)}</dd>
        <dt className="text-ink-3">{m.details.size}</dt>
        <dd className="tabular-nums text-ink-2">
          {m.details.sizeValue(formatBytes(root.logical_bytes), formatBytes(root.allocated_bytes))}
        </dd>
        <dt className="text-ink-3">{m.details.reclaimable}</dt>
        <dd className="tabular-nums text-ink-2">
          {root.reclaimable_exact ? reclaimable : m.details.atLeast(reclaimable)}
        </dd>
      </dl>
      <p
        className={`mt-1.5 ${root.coverage_complete ? "text-ink-3" : "text-amber-800 dark:text-amber-300"}`}
      >
        {root.coverage_complete
          ? m.details.complete
          : m.details.incomplete(root.denied, root.failed, root.not_scanned)}
      </p>
    </li>
  );
}

/** Per-folder numbers, for those who want them, and the way to scan other folders; closed by default. */
export function ScanDetails({
  scan,
  onStartOver,
}: {
  readonly scan: ScanView;
  readonly onStartOver: () => void;
}) {
  const { m } = useI18n();
  return (
    <div>
      <Disclosure summary={<span className="text-ink-2">{m.details.title}</span>}>
        <p className="text-[12px] text-ink-3">{m.details.entries(scan.entries)}</p>
        <ul className="mt-1 divide-y divide-line">
          {scan.roots.map((root) => (
            <RootDetails key={root.root} root={root} />
          ))}
        </ul>
        <button
          type="button"
          onClick={onStartOver}
          className={`${buttonClass("quiet", "sm")} -ml-2`}
        >
          {m.details.scanOther}
        </button>
      </Disclosure>
    </div>
  );
}
