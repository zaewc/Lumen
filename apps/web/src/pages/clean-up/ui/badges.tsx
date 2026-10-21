import type { GroupKindView, VerdictView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { AppIcon, CodeIcon, FolderIcon, LayersIcon } from "@/shared/ui";

function verdictClass(verdict: VerdictView): string {
  switch (verdict) {
    case "quarantine":
      return "bg-accent-soft text-accent-ink";
    case "review":
      return "bg-amber-500/15 text-amber-800 dark:bg-amber-400/15 dark:text-amber-300";
    case "keep":
      return "bg-fill-strong text-ink-2";
  }
}

/** The policy's verdict, as a small coloured label. */
export function VerdictBadge({ verdict }: { readonly verdict: VerdictView }) {
  const { m } = useI18n();
  return (
    <span
      className={`inline-flex shrink-0 items-center rounded-[5px] px-1.5 py-px text-[11px] leading-4 font-semibold ${verdictClass(verdict)}`}
    >
      {m.results.verdict[verdict]}
    </span>
  );
}

function kindClass(kind: GroupKindView): string {
  switch (kind) {
    case "developer":
      return "bg-linear-to-b from-indigo-400 to-indigo-600";
    case "app_cache":
      return "bg-linear-to-b from-sky-400 to-sky-600";
    case "app_data":
      return "bg-linear-to-b from-violet-400 to-violet-600";
    case "other":
      return "bg-linear-to-b from-neutral-400 to-neutral-500";
  }
}

function KindGlyph({ kind }: { readonly kind: GroupKindView }) {
  switch (kind) {
    case "developer":
      return <CodeIcon className="size-[17px]" />;
    case "app_cache":
      return <LayersIcon className="size-[17px]" />;
    case "app_data":
      return <AppIcon className="size-[17px]" />;
    case "other":
      return <FolderIcon className="size-[17px]" />;
  }
}

/** The kind of group an item is in, as a small icon tile. */
export function KindIcon({ kind }: { readonly kind: GroupKindView }) {
  return (
    <span
      aria-hidden="true"
      className={`flex size-8 shrink-0 items-center justify-center rounded-lg text-white shadow-sm shadow-black/10 ${kindClass(kind)}`}
    >
      <KindGlyph kind={kind} />
    </span>
  );
}
