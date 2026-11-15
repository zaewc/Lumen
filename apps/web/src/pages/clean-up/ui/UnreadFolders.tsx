import type { ScanView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { Disclosure } from "@/shared/ui";
import { unlistedCount, unreadGroups } from "../model/unread";

/**
 * Which folders the scan could not read, grouped by why, each with what a
 * person can do about it (ADR-0008). Asked for only once opened; the paths
 * are for reading, never acted on, and never sent to AI review.
 */
export function UnreadFolders({ scan }: { readonly scan: ScanView }) {
  const { m } = useI18n();
  const ask = useTransport().unreadFolders;
  const [opened, setOpened] = useState(false);
  const incomplete = scan.roots.some((root) => !root.coverage_complete);
  const unread = useQuery({
    queryKey: ["unread-folders", scan.scan],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask(scan.scan)),
    enabled: ask !== undefined && incomplete && opened,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
    refetchOnWindowFocus: false,
  });
  if (ask === undefined || !incomplete) return null;
  const t = m.details.unread;
  return (
    <Disclosure
      className="mt-2"
      summary={<span className="text-attention">{t.title}</span>}
      onOpen={() => setOpened(true)}
    >
      {unread.isPending && <p className="text-[12px] text-ink-3">{t.loading}</p>}
      {unread.isError && <p className="text-[12px] text-attention">{t.failed}</p>}
      {unread.data && (
        <div className="space-y-3">
          {unreadGroups(unread.data).map((group) => (
            <section key={group.reason}>
              <p className="text-[12px] text-ink-2">{t.reasons[group.reason]}</p>
              <ul className="mt-1 space-y-0.5">
                {group.paths.map((path) => (
                  <li key={path} className="font-mono text-[12px] break-all text-ink select-text">
                    {path}
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {unlistedCount(unread.data) !== "0" && (
            <p className="text-[12px] text-ink-3">{t.more(unlistedCount(unread.data))}</p>
          )}
        </div>
      )}
    </Disclosure>
  );
}
