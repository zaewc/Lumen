import type { ScanView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";

/** The roots typed into the form, one per line, blank lines ignored. */
export function rootsOf(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** Chooses folders and scans them, read-only. */
export function ScanForm({ onScanned }: { readonly onScanned: (scan: ScanView) => void }) {
  const { m } = useI18n();
  const transport = useTransport();
  const [text, setText] = useState("");
  const scan = useMutation({
    mutationFn: (roots: string[]) => transport.scan({ roots }),
    onSuccess: onScanned,
  });
  const roots = rootsOf(text);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    scan.mutate(roots);
  };
  return (
    <form onSubmit={submit} className="space-y-2">
      <label htmlFor="roots" className="block text-sm font-medium">
        {m.scan.rootsLabel}
      </label>
      <textarea
        id="roots"
        value={text}
        onChange={(event) => setText(event.target.value)}
        rows={3}
        className="w-full rounded border border-neutral-300 p-2 font-mono text-sm"
        placeholder="/Users/you/Library/Developer"
      />
      <p className="text-xs text-neutral-600">{m.scan.readOnly}</p>
      <div className="flex flex-wrap gap-2">
        <button
          type="submit"
          disabled={roots.length === 0 || scan.isPending}
          className="rounded bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40"
        >
          {scan.isPending ? m.scan.scanning : m.scan.submit}
        </button>
        {/* No roots: the core scans the whole startup disk. */}
        <button
          type="button"
          onClick={() => scan.mutate([])}
          disabled={scan.isPending}
          aria-describedby="whole-disk-hint"
          className="rounded border border-neutral-400 px-3 py-1.5 text-sm disabled:opacity-40"
        >
          {m.scan.wholeDisk}
        </button>
      </div>
      <p id="whole-disk-hint" className="text-xs text-neutral-600">
        {m.scan.wholeDiskHint}
      </p>
      {scan.isError && <p className="text-sm text-red-700">{String(scan.error)}</p>}
    </form>
  );
}
