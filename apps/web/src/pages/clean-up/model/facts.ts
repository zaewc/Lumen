import type { SoftwareItemView } from "@lumen/api-client";
import type { Messages } from "@/shared/i18n";

/** One thing Lumen observed about removing an app, and where it read it. */
export interface Fact {
  readonly text: string;
  readonly source: string;
}

/**
 * What Lumen itself observed about removing `item` (#850), each with its
 * source, from the API's `facts` and linked background items only. AI
 * review's note is never an input: these are shown apart from it, as
 * observations (ADR-0061 rule 5). Unknown is said only where it matters, and
 * never as "none" (ADR-0008).
 */
export function removalFacts(m: Messages, item: SoftwareItemView): readonly Fact[] {
  const f = m.apps.facts;
  const facts = item.facts;
  // A Windows program's record is its uninstall entry; an app a package
  // installed is described by that package's record.
  const record =
    facts.installed_by !== null
      ? m.apps.origins[facts.installed_by]
      : item.origin === "windows_program"
        ? f.uninstallEntry
        : m.apps.origins[item.origin];
  const from = (text: string, source = record): Fact => ({ text, source: f.source(source) });
  const out: Fact[] = [];
  if (facts.installed_by !== null) out.push(from(f.installedBy));
  if (facts.mechanism !== null) out.push(from(f.mechanisms[facts.mechanism]));
  if (facts.needs_administrator) out.push(from(f.administrator));
  const neededBy = facts.needed_by;
  if (neededBy !== null) {
    if (neededBy.count > 0) out.push(from(f.neededBy(neededBy.count, neededBy.complete)));
    else out.push(from(neededBy.complete ? f.neededByNone : f.neededByUnknown));
  }
  if (facts.needs !== null && facts.needs.count > 0) out.push(from(f.needs(facts.needs.count)));
  const verified = item.services.filter((s) => s.verified).length;
  if (verified > 0) out.push(from(f.background(verified), f.backgroundRecords));
  return out;
}
