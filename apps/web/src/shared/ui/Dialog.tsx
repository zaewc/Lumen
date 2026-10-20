import { type ReactNode, type RefObject, useEffect, useRef } from "react";
import { dialogChange } from "./modal";

/**
 * A modal dialog on the native `<dialog>` element: `showModal()` makes the
 * rest of the page inert, so focus stays inside it. On opening, focus goes
 * to `initialFocus` (or the dialog); on closing, back to `returnFocus` (or
 * whatever had it before). Escape closes it unless `dismissible` is false.
 *
 * `onClose` means "the dialog was dismissed": the owner must set `open` to
 * false. It is also called if the browser closes the dialog on its own.
 */
export function Dialog({
  open,
  onClose,
  labelledBy,
  describedBy,
  dismissible = true,
  initialFocus,
  returnFocus,
  children,
}: {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly labelledBy: string;
  readonly describedBy?: string;
  readonly dismissible?: boolean;
  readonly initialFocus?: RefObject<HTMLElement | null>;
  readonly returnFocus?: RefObject<HTMLElement | null>;
  readonly children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  // The latest props, for the element's own events.
  const latest = useRef({ open, onClose, returnFocus });
  latest.current = { open, onClose, returnFocus };

  useEffect(() => {
    const element = dialog.current;
    if (element === null) {
      return;
    }
    switch (dialogChange(open, element.open)) {
      case "show": {
        const focused = document.activeElement;
        opener.current = focused instanceof HTMLElement ? focused : null;
        element.showModal();
        (initialFocus?.current ?? element).focus();
        break;
      }
      case "close":
        element.close();
        break;
      case "none":
        break;
    }
  }, [open, initialFocus]);

  const closed = () => {
    const target = latest.current.returnFocus?.current ?? opener.current;
    if (target?.isConnected) {
      target.focus();
    }
    if (latest.current.open) {
      latest.current.onClose();
    }
  };

  return (
    <dialog
      ref={dialog}
      aria-modal="true"
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      tabIndex={-1}
      onCancel={(event) => {
        event.preventDefault();
        if (dismissible) {
          latest.current.onClose();
        }
      }}
      onClose={closed}
      className="m-auto max-h-[calc(100dvh-2rem)] w-[min(32rem,calc(100vw-2rem))] overflow-y-auto rounded-2xl bg-white p-6 text-neutral-900 shadow-xl outline-none backdrop:bg-neutral-950/40 dark:border dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100 dark:backdrop:bg-neutral-950/70"
    >
      {children}
    </dialog>
  );
}
