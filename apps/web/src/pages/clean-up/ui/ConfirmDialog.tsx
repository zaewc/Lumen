import type { PlanView } from "@lumen/api-client";
import { type RefObject, useId, useRef } from "react";
import { formatDateTime, useI18n } from "@/shared/i18n";
import { formatBytes, sumBytes } from "@/shared/lib";
import { Bytes, buttonClass, CheckIcon, Dialog, Disclosure, ErrorNotice } from "@/shared/ui";
import { planRows } from "../model/confirm";

/**
 * Asks before moving a plan's items to quarantine. Pressing "Move to
 * Quarantine" is the explicit confirmation of exactly this plan (its hash
 * is sent with it, ADR-0034); the hash itself is only under "Details".
 */
export function ConfirmDialog({
  open,
  plan,
  busy,
  error,
  onConfirm,
  onClose,
  returnFocus,
}: {
  readonly open: boolean;
  readonly plan: PlanView;
  readonly busy: boolean;
  readonly error: unknown;
  readonly onConfirm: () => void;
  readonly onClose: () => void;
  readonly returnFocus: RefObject<HTMLElement | null>;
}) {
  const { locale, m } = useI18n();
  const title = useId();
  const description = useId();
  const dismiss = useRef<HTMLButtonElement>(null);
  const empty = plan.items.length === 0;
  const size = formatBytes(sumBytes(plan.items.map((item) => item.reclaimable_bytes)));
  return (
    <Dialog
      open={open}
      onClose={onClose}
      labelledBy={title}
      {...(empty ? {} : { describedBy: description })}
      dismissible={!busy}
      initialFocus={dismiss}
      returnFocus={returnFocus}
    >
      <div className="flex max-h-[calc(100dvh-5rem)] flex-col">
        <h2 id={title} className="shrink-0 text-title text-ink">
          {empty ? m.confirm.nothingLeft : m.confirm.title(plan.items.length, size)}
        </h2>
        <div className="-mx-1 mt-3 min-h-0 overflow-x-hidden overflow-y-auto px-1">
          {!empty && (
            <ul id={description} className="space-y-2.5 text-[14px] leading-relaxed text-ink-2">
              <li className="flex gap-2.5">
                <CheckIcon className="mt-0.5 size-[18px] shrink-0 text-ink-3" />
                <span>{m.confirm.checked}</span>
              </li>
              <li className="flex gap-2.5">
                <CheckIcon className="mt-0.5 size-[18px] shrink-0 text-ink-3" />
                <span>{m.confirm.restorable}</span>
              </li>
            </ul>
          )}
          {!empty && (
            <ul
              aria-label={m.confirm.items}
              className="mt-4 divide-y divide-line border-y border-line text-[14px]"
            >
              {planRows(plan.items).map((row) => (
                <li key={row.id} className="flex items-baseline gap-3 py-2" title={row.path}>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-ink">{row.name}</span>
                    <span className="block truncate text-[12px] text-ink-3">{row.folder}</span>
                  </span>
                  <span className="shrink-0 tabular-nums text-ink">
                    <Bytes value={row.bytes} />
                  </span>
                </li>
              ))}
            </ul>
          )}
          <Disclosure
            className="mx-2 mt-4"
            summary={<span className="text-[13px] text-ink-2">{m.common.details}</span>}
          >
            <div className="space-y-1 text-[12px] text-ink-2">
              <p>{m.confirm.planMade(formatDateTime(locale, plan.created_at))}</p>
              <p>
                {m.confirm.planHash}: <code className="font-mono break-all">{plan.hash}</code>
              </p>
            </div>
          </Disclosure>
          {error !== null && (
            <div className="mt-4">
              <ErrorNotice message={m.confirm.failed} error={error} />
            </div>
          )}
        </div>
        <div className="mt-6 flex shrink-0 flex-col-reverse gap-2 min-[480px]:flex-row min-[480px]:justify-end">
          <button
            ref={dismiss}
            type="button"
            onClick={onClose}
            disabled={busy}
            className={buttonClass("secondary")}
          >
            {m.common.close}
          </button>
          {!empty && (
            <button
              type="button"
              onClick={onConfirm}
              disabled={busy}
              className={buttonClass("primary")}
            >
              {busy ? m.confirm.moving : m.confirm.move}
            </button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
