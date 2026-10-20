import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { baseName } from "@/shared/lib";
import { buttonClass, Card, ErrorNotice, InfoTip, Notice } from "@/shared/ui";
import { doneText, doneWell, type LastAction } from "../lib/outcome";
import { HeldRow } from "./HeldRow";

/** Like a trash can: what quarantine holds, to restore or remove for good. */
export function QuarantinePage() {
  const { m } = useI18n();
  const transport = useTransport();
  const held = useQuery({ queryKey: ["held"], queryFn: () => transport.held() });
  // Shown above the list, so it stays after a restored item leaves the list.
  const [last, setLast] = useState<LastAction | null>(null);
  const items = held.data ?? [];
  return (
    <main className="mx-auto max-w-3xl space-y-4 px-4 py-8 sm:px-6 sm:py-12">
      <div className="px-1">
        <div className="flex items-center gap-2">
          <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
            {m.quarantine.title}
          </h1>
          <InfoTip term="quarantine" />
        </div>
        {items.length > 0 && (
          <p className="mt-1 text-neutral-600">{m.quarantine.count(items.length)}</p>
        )}
      </div>
      <div role="status">
        {last && (
          <Notice tone={doneWell(last.done) ? "success" : "warning"}>
            <p>
              {last.original !== null && (
                <span className="font-semibold">{baseName(last.original)}: </span>
              )}
              {doneText(m, last.done)}
            </p>
          </Notice>
        )}
      </div>
      {held.isPending && <p className="px-1 text-neutral-500">{m.common.loading}</p>}
      {held.isError && <ErrorNotice message={m.quarantine.loadFailed} error={held.error} />}
      {held.isSuccess && items.length === 0 && (
        <Card className="py-12 text-center sm:py-14">
          <p className="text-lg font-semibold">{m.quarantine.empty}</p>
          <p className="mx-auto mt-2 max-w-sm text-neutral-600">{m.quarantine.emptyHint}</p>
          <Link to="/" className={`${buttonClass("secondary")} mt-6`}>
            {m.quarantine.goCleanUp}
          </Link>
        </Card>
      )}
      {items.length > 0 && (
        <Card className="py-2 sm:py-2">
          <div className="flex items-start gap-2 border-b border-neutral-100 py-3 text-sm text-neutral-600">
            <p className="flex-1">{m.quarantine.intro}</p>
            <InfoTip term="restore" />
          </div>
          <ul className="divide-y divide-neutral-100">
            {items.map((item) => (
              <HeldRow key={item.operation} item={item} onDone={setLast} />
            ))}
          </ul>
        </Card>
      )}
    </main>
  );
}
