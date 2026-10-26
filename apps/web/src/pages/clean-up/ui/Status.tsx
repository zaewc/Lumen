import type { Status } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";

/**
 * Only what needs the user's attention stands out: an item to review, and
 * one to clean up with its own command.
 */
export function statusClass(status: Status): string {
  switch (status) {
    case "quarantine":
      return "text-ink-2";
    case "command":
      return "font-medium text-accent-ink";
    case "review":
      return "font-medium text-amber-700 dark:text-amber-400";
    case "keep":
      return "text-ink-3";
  }
}

/** An item's status, as a word in its colour. */
export function StatusText({ status }: { readonly status: Status }) {
  const { m } = useI18n();
  return <span className={statusClass(status)}>{m.results.status[status]}</span>;
}
