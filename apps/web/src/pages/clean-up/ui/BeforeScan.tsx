import type { SoftwareItemView } from "@lumen/api-client";
import { useMemo, useState } from "react";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { title } from "../model/apps-list";
import { listedOwners, type OwnerOrder, ownersOf } from "../model/owners";
import { CandidateList, type Check } from "./CandidateList";
import { MoveFlow, MoveResult } from "./MoveFlow";
import { OwnerControls } from "./OwnerControls";
import { SoftwareDetails } from "./SoftwareDetails";
import { useSoftware } from "./useSoftware";

/**
 * Before a scan (ADR-0069): the same list as after one, with what is
 * installed alone, sized by the latest scan where it measured them, so an
 * app can be found and moved without scanning.
 */
export function BeforeScan({ check }: { readonly check: Check }) {
  const { m } = useI18n();
  const software = useSoftware();
  const [query, setQuery] = useState("");
  const [order, setOrder] = useState<OwnerOrder>("size");
  const [info, setInfo] = useState<SoftwareItemView | null>(null);
  const [app, setApp] = useState<SoftwareItemView | null>(null);
  const items = software.list?.items;
  const { owners, small, system } = useMemo(
    () => listedOwners(ownersOf([], items ?? []), { apps: true, query, order }),
    [items, query, order],
  );
  if (!software.available) return null;
  return (
    <section className="space-y-3 pt-8" aria-label={m.apps.title}>
      {software.failed && (
        <ErrorNotice message={m.apps.loadFailed} error={software.error}>
          <button
            type="button"
            className={buttonClass("secondary", "sm")}
            onClick={software.readAgain}
          >
            {m.common.tryAgain}
          </button>
        </ErrorNotice>
      )}
      <MoveResult
        name={(key) => {
          const found = items?.find((item) => item.key === key);
          return found === undefined ? null : title(found);
        }}
      />
      <OwnerControls
        query={query}
        onQuery={setQuery}
        order={order}
        onOrder={setOrder}
        software={software}
      />
      {items !== undefined && (
        <CandidateList
          owners={owners}
          folded={{ small, system }}
          check={check}
          onOpen={() => {}}
          onOpenApp={setApp}
          onInfo={setInfo}
          lens={null}
        />
      )}
      {info !== null && (
        <SoftwareDetails
          item={info}
          onClose={() => setInfo(null)}
          onMove={(item) => {
            setInfo(null);
            setApp(item);
          }}
        />
      )}
      {app !== null && <MoveFlow item={app} onClose={() => setApp(null)} />}
    </section>
  );
}
