import { useQuery } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";

/** The start page: what quarantine holds right now. */
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
        {held.isPending && <p className="mt-2 text-sm">Loading…</p>}
        {held.isError && <p className="mt-2 text-sm text-red-700">Could not reach the core.</p>}
        {held.data?.length === 0 && <p className="mt-2 text-sm">Nothing is held.</p>}
        <ul className="mt-2 space-y-1">
          {held.data?.map((item) => (
            <li key={item.operation} className="text-sm">
              {item.original ?? "(unknown path)"}
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
