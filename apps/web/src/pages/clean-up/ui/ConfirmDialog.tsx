import type { PlanView } from "@lumen/api-client";
import { type RefObject, useId, useRef } from "react";
import { formatDateTime, useI18n } from "@/shared/i18n";
import { formatBytes, sumBytes } from "@/shared/lib";
import { buttonClass, CheckIcon, Dialog, Disclosure, ErrorNotice } from "@/shared/ui";

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
      describedBy={description}
      dismissible={!busy}
      initialFocus={dismiss}
      returnFocus={returnFocus}
    >
      <h2 id={title} className="text-[15px] leading-snug font-semibold text-ink">
        {empty ? m.confirm.nothingLeft : m.confirm.title(plan.items.length, size)}
      </h2>
      {!empty && (
        <ul id={description} className="mt-3 space-y-2.5 text-[13px] leading-snug text-ink-2">
          <li className="flex gap-2.5">
            <CheckIcon className="size-[18px] text-emerald-600 dark:text-emerald-400" />
            <span>{m.confirm.checked}</span>
          </li>
          <li className="flex gap-2.5">
            <CheckIcon className="size-[18px] text-emerald-600 dark:text-emerald-400" />
            <span>{m.confirm.restorable}</span>
          </li>
        </ul>
      )}
      <Disclosure
        className="mt-4"
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
      <div className="mt-6 flex flex-col-reverse gap-2 min-[480px]:flex-row min-[480px]:justify-end">
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
    </Dialog>
  );
}
