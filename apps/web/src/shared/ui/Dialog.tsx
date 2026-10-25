import { type ReactNode, type RefObject, useEffect, useRef } from "react";
import { dialogChange } from "./modal";

/** A dialog in the middle of the window, or a sheet along its right edge. */
export type DialogVariant = "alert" | "sheet";

function variantClass(variant: DialogVariant): string {
  switch (variant) {
    case "alert":
      return "m-auto max-h-[calc(100dvh-2rem)] w-[min(30rem,calc(100vw-2rem))] overflow-y-auto rounded-xl bg-raised p-5 shadow-xl ring-1 ring-line animate-[pop-in_160ms_ease-out] motion-reduce:animate-none backdrop:bg-black/25 dark:backdrop:bg-black/50";
    case "sheet":
      return "fixed inset-0 m-0 h-dvh max-h-none w-screen max-w-none overflow-hidden bg-transparent p-0 backdrop:bg-black/15 dark:backdrop:bg-black/40";
  }
}

/**
 * A modal dialog on the native `<dialog>` element: `showModal()` makes the
 * rest of the page inert, so focus stays inside it. On opening, focus goes
 * to `initialFocus` (or the dialog); on closing, back to `returnFocus` (or
 * whatever had it before). Escape closes it unless `dismissible` is false.
 *
 * `onClose` means "the dialog was dismissed": the owner must set `open` to
 * false. It is also called if the browser closes the dialog on its own. A
 * sheet is also dismissed by a click on the dimmed window beside it.
 */
export function Dialog({
  open,
  onClose,
  labelledBy,
  describedBy,
  dismissible = true,
  initialFocus,
  returnFocus,
  variant = "alert",
  children,
}: {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly labelledBy: string;
  readonly describedBy?: string;
  readonly dismissible?: boolean;
  readonly initialFocus?: RefObject<HTMLElement | null>;
  readonly returnFocus?: RefObject<HTMLElement | null>;
  readonly variant?: DialogVariant;
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
      className={`text-ink outline-none ${variantClass(variant)}`}
    >
      {variant === "sheet" ? (
        <div className="flex h-full">
          {/* The dimmed window beside the sheet: a click there closes it, as
              Escape and the sheet's own close button do for the keyboard. */}
          <button
            type="button"
            tabIndex={-1}
            aria-hidden="true"
            disabled={!dismissible}
            onClick={() => latest.current.onClose()}
            className="hidden min-w-0 flex-1 cursor-default min-[720px]:block"
          />
          <div className="h-full w-full bg-raised shadow-2xl animate-[sheet-in_200ms_ease-out] motion-reduce:animate-none min-[720px]:w-[26rem] min-[720px]:border-l min-[720px]:border-line">
            {children}
          </div>
        </div>
      ) : (
        children
      )}
    </dialog>
  );
}
