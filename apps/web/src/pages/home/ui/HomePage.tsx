import { useQuery } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { InfoTip } from "@/shared/ui";
import { HeldItem } from "./HeldItem";

/** The start page: what quarantine holds, to restore or remove for good. */
export function HomePage() {
  const { m } = useI18n();
  const transport = useTransport();
  const held = useQuery({ queryKey: ["held"], queryFn: () => transport.held() });
  return (
    <main className="mx-auto max-w-3xl p-8">
      <h1 className="text-2xl font-semibold">Lumen</h1>
      <p className="mt-2 text-sm text-neutral-600">{m.home.tagline}</p>
      <section className="mt-8">
        <div className="flex items-center gap-2">
          <h2 className="text-lg font-medium">{m.home.heading}</h2>
          <InfoTip term="quarantine" />
        </div>
        <p className="mt-1 text-xs text-neutral-600">{m.home.spaceNote}</p>
        {held.isPending && <p className="mt-2 text-sm">{m.home.loading}</p>}
        {held.isError && <p className="mt-2 text-sm text-red-700">{String(held.error)}</p>}
        {held.data?.length === 0 && <p className="mt-2 text-sm">{m.home.empty}</p>}
        <ul className="mt-3 space-y-2">
          {held.data?.map((item) => (
            <HeldItem key={item.operation} item={item} />
          ))}
        </ul>
      </section>
    </main>
  );
}
