import type { VerdictView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";

/** Only what needs the user's attention stands out: an item to review. */
export function verdictClass(verdict: VerdictView): string {
  switch (verdict) {
    case "quarantine":
      return "text-ink-2";
    case "review":
      return "font-medium text-amber-700 dark:text-amber-400";
    case "keep":
      return "text-ink-3";
  }
}

/** The policy's verdict, as a word in its colour. */
export function VerdictText({ verdict }: { readonly verdict: VerdictView }) {
  const { m } = useI18n();
  return <span className={verdictClass(verdict)}>{m.results.verdict[verdict]}</span>;
}
