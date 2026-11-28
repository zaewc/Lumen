import type { SoftwareItemView } from "@lumen/api-client";
import { useId } from "react";
import { softwareNoteText, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, Dialog } from "@/shared/ui";
import { title } from "../model/apps-list";
import { removalFacts } from "../model/facts";
import { AppIcon } from "./AppIcon";
import { HomebrewRemoval, isHomebrew } from "./HomebrewRemoval";
import { SoftwareRelations } from "./SoftwareRelations";

/**
 * What Lumen knows about one installed app or package (ADR-0060, ADR-0061):
 * where it came from, where it is, what the scan measured, what Lumen found
 * about removing it, what it needs and what needs it (ADR-0064), and AI
 * review's note, labelled as AI's guess. A
 * user-owned macOS app can be moved from here through its own dialog
 * (ADR-0062), and a Homebrew package removed through Homebrew when its
 * preview allows it (ADR-0060); nothing else is removed.
 */
export function SoftwareDetails({
  item,
  onClose,
  onMove,
}: {
  readonly item: SoftwareItemView;
  readonly onClose: () => void;
  /** Opens the app's move dialog; absent where apps cannot be moved. */
  readonly onMove?: (item: SoftwareItemView) => void;
}) {
  const { m } = useI18n();
  const a = m.apps;
  const heading = useId();
  const details = [
    a.origins[item.origin],
    item.name !== null ? item.id : null,
    item.version,
    item.vendor,
  ]
    .filter((part): part is string => part !== null)
    .join(" · ");
  const facts = removalFacts(m, item);
  const size = item.size;
  const movable = onMove !== undefined && item.origin === "mac_app" && !item.protected;
  return (
    <Dialog open onClose={onClose} labelledBy={heading}>
      <div className="space-y-4">
        <div className="flex items-start gap-3">
          <AppIcon item={item} />
          <div className="min-w-0 space-y-0.5">
            <h2 id={heading} className="text-section text-ink">
              {title(item)}
            </h2>
            <p className="text-[12px] text-ink-2">{details}</p>
            {item.location !== null && (
              <p className="font-mono text-[11px] break-all text-ink-3">{item.location}</p>
            )}
            {size !== null && size.kind !== "not_scanned" && (
              <p className="text-[12px] tabular-nums text-ink-2">
                {size.kind === "at_least"
                  ? m.details.atLeast(formatBytes(size.bytes))
                  : formatBytes(size.bytes)}
              </p>
            )}
          </div>
        </div>
        {item.protected && (
          <p title={a.protectedTip} className="text-[13px] text-ink-2">
            {a.protected}
          </p>
        )}
        {facts.length > 0 && (
          <div>
            <p className="text-[11px] font-medium text-ink-3">{a.facts.title}</p>
            <ul className="text-[13px] text-ink-2">
              {facts.map((fact) => (
                <li key={fact.text}>
                  {fact.text} <span className="text-ink-3">· {fact.source}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
        <SoftwareRelations item={item} />
        {item.ai_note !== null && (
          <div>
            <p className="text-[13px] text-ink-2">{softwareNoteText(m, item.ai_note)}</p>
            <p className="text-[11px] text-ink-3">{a.ai.source}</p>
          </div>
        )}
        {isHomebrew(item) && <HomebrewRemoval item={item} />}
        <div className="flex justify-end gap-2">
          <button type="button" onClick={onClose} className={buttonClass("secondary")}>
            {m.common.close}
          </button>
          {movable && (
            <button type="button" onClick={() => onMove(item)} className={buttonClass("primary")}>
              {a.move.action}
            </button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
