import { useQuery } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";
import { HeldItem } from "./HeldItem";

/** The start page: what quarantine holds, to restore or remove for good. */
export function HomePage() {
  const transport = useTransport();
  const held = useQuery({ queryKey: ["held"], queryFn: () => transport.held() });
  return (
    <main className="mx-auto max-w-3xl p-8">
      <h1 className="text-2xl font-semibold">Lumen</h1>
      <p className="mt-2 text-sm text-neutral-600">
        Storage cleanup that keeps everything reversible until you decide otherwise.
      </p>
      <section className="mt-8">
        <h2 className="text-lg font-medium">In quarantine</h2>
        <p className="mt-1 text-xs text-neutral-600">
          Quarantined items still use space; it is freed only when an item is removed for good.
        </p>
        {held.isPending && <p className="mt-2 text-sm">Loading…</p>}
        {held.isError && <p className="mt-2 text-sm text-red-700">{String(held.error)}</p>}
        {held.data?.length === 0 && <p className="mt-2 text-sm">Nothing is held.</p>}
        <ul className="mt-3 space-y-2">
          {held.data?.map((item) => (
            <HeldItem key={item.operation} item={item} />
          ))}
        </ul>
      </section>
    </main>
  );
}
