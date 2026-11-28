import type { SoftwareItemView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { itemRelations, type RelationEntry, type RelationSection } from "../model/relations";

/**
 * What a package needs and what needs it, from the package managers' own
 * records (ADR-0064), in its details sheet only: the list and its selection
 * never change for it. Each entry names its record and whether it is proven
 * or one of several candidates; records Lumen could not read are said to be
 * unknown, never "none" (ADR-0008).
 */
export function SoftwareRelations({ item }: { readonly item: SoftwareItemView }) {
  const { m } = useI18n();
  const r = m.apps.relations;
  const { needs, neededBy } = itemRelations(item);
  if (needs?.kind === "unread" && neededBy?.kind === "unread") {
    return <p className="text-[13px] text-ink-2">{r.unread}</p>;
  }
  return (
    <>
      {needs !== null && <Section title={r.needs} section={needs} />}
      {neededBy !== null && <Section title={r.neededBy} section={neededBy} />}
    </>
  );
}

function Section({
  title,
  section,
}: {
  readonly title: string;
  readonly section: RelationSection;
}) {
  const { m } = useI18n();
  const r = m.apps.relations;
  const heading =
    section.kind === "entries" && section.atLeast
      ? `${title} · ${r.atLeast(section.entries.length)}`
      : title;
  return (
    <div>
      <p className="text-[11px] font-medium text-ink-3">{heading}</p>
      {section.kind === "unread" ? (
        <p className="text-[13px] text-ink-2">{r.unread}</p>
      ) : (
        <ul className="text-[13px] text-ink-2">
          {section.entries.map((entry) => (
            <Entry key={`${entry.record}:${entry.name}`} entry={entry} />
          ))}
        </ul>
      )}
    </div>
  );
}

function Entry({ entry }: { readonly entry: RelationEntry }) {
  const { m } = useI18n();
  const r = m.apps.relations;
  const certainty =
    entry.certainty === "verified"
      ? r.verified
      : entry.certainty === "candidate"
        ? r.candidate
        : null;
  return (
    <li>
      <span className="font-mono text-[12px] break-all">{entry.name}</span>{" "}
      <span className="text-ink-3">
        · {m.apps.facts.source(r.records[entry.record])}
        {certainty !== null && ` · ${certainty}`}
      </span>
    </li>
  );
}
