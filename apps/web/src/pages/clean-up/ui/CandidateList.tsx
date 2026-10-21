import type { CandidateView } from "@lumen/api-client";
import { type ReactNode, useId } from "react";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes, parentPath } from "@/shared/lib";
import { ChevronIcon } from "@/shared/ui";
import {
  type KindSection,
  presentVerdicts,
  type SourceGroup,
  type VerdictCounts,
} from "../model/sections";
import { KindIcon, VerdictBadge } from "./badges";

/** "2 proposed · 1 to review", for a section's header. */
function VerdictSummary({ counts }: { readonly counts: VerdictCounts }) {
  const { m } = useI18n();
  return (
    <>
      {presentVerdicts(counts)
        .map((verdict) => m.results.verdictCount[verdict](counts[verdict]))
        .join(" · ")}
    </>
  );
}

/** One candidate: what it is, how large, the verdict; it opens the details. */
function Row({
  candidate,
  subtitle,
  onOpen,
}: {
  readonly candidate: CandidateView;
  readonly subtitle: string;
  readonly onOpen: (candidate: CandidateView) => void;
}) {
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(candidate)}
        className="flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-fill focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent active:bg-fill-strong motion-reduce:transition-none"
      >
        <KindIcon kind={candidate.group.kind} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[14px] font-medium text-ink">
            {baseName(candidate.path)}
          </span>
          <span className="mt-0.5 flex min-w-0 items-center gap-1.5">
            <VerdictBadge verdict={candidate.verdict} />
            <span className="min-w-0 truncate text-[12px] text-ink-3">{subtitle}</span>
          </span>
        </span>
        <span className="shrink-0 text-[13px] tabular-nums text-ink-2">
          {formatBytes(candidate.reclaimable_bytes)}
        </span>
        <ChevronIcon className="text-ink-3" />
      </button>
    </li>
  );
}

function Rows({
  group,
  subtitle,
  onOpen,
}: {
  readonly group: SourceGroup;
  readonly subtitle: (candidate: CandidateView) => string;
  readonly onOpen: (candidate: CandidateView) => void;
}) {
  return (
    <ul className="divide-y divide-line">
      {group.items.map((candidate) => (
        <Row
          key={candidate.path}
          candidate={candidate}
          subtitle={subtitle(candidate)}
          onOpen={onOpen}
        />
      ))}
    </ul>
  );
}

/** One kind of group: its header with totals, then its items (by tool, for developer tools). */
function Section({
  section,
  onOpen,
}: {
  readonly section: KindSection;
  readonly onOpen: (candidate: CandidateView) => void;
}) {
  const { m } = useI18n();
  const heading = useId();
  const byTool = section.kind === "developer";
  // Under a tool's header the tool is known, so the row names its folder;
  // elsewhere it names the app it belongs to, when known.
  const subtitle = (candidate: CandidateView) =>
    (byTool ? null : candidate.group.source) ?? parentPath(candidate.path);
  let body: ReactNode;
  if (byTool) {
    body = section.sources.map((group) => (
      <div key={group.source ?? ""} className="border-t border-line first:border-t-0">
        <h4 className="flex items-baseline justify-between gap-3 bg-fill px-3.5 py-1.5 text-[12px] font-semibold text-ink-2">
          <span className="truncate">{group.source ?? m.results.otherTools}</span>
          <span className="shrink-0 font-normal tabular-nums text-ink-3">
            {m.results.section(group.items.length, formatBytes(group.bytes))}
          </span>
        </h4>
        <Rows group={group} subtitle={subtitle} onOpen={onOpen} />
      </div>
    ));
  } else {
    body = section.sources.map((group) => (
      <Rows key={group.source ?? ""} group={group} subtitle={subtitle} onOpen={onOpen} />
    ));
  }
  return (
    <section aria-labelledby={heading}>
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 px-1 pb-2">
        <h3 id={heading} className="text-[14px] font-semibold text-ink">
          {m.results.groups[section.kind]}
          <span className="ml-2 text-[12px] font-normal tabular-nums text-ink-3">
            {m.results.section(section.count, formatBytes(section.bytes))}
          </span>
        </h3>
        <p className="text-[12px] tabular-nums text-ink-2">
          <VerdictSummary counts={section.verdicts} />
        </p>
      </div>
      <div className="overflow-hidden rounded-xl border border-line bg-surface">{body}</div>
    </section>
  );
}

/** The candidates in sections by group. */
export function CandidateList({
  sections,
  onOpen,
}: {
  readonly sections: readonly KindSection[];
  readonly onOpen: (candidate: CandidateView) => void;
}) {
  return (
    <div className="space-y-6">
      {sections.map((section) => (
        <Section key={section.kind} section={section} onOpen={onOpen} />
      ))}
    </div>
  );
}
