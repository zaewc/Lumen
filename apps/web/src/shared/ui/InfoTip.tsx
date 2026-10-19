import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { type GlossaryTerm, useI18n } from "@/shared/i18n";
import { nextTip, panelShift, type TipEvent } from "./tip";

/**
 * An "i" button that shows what a term means, from the glossary. It is a
 * real button (click, Enter or Space toggles; Escape or a click outside
 * closes), so it works the same with touch, mouse and keyboard. Its panel
 * opens under the button and slides left as needed to stay on screen.
 */
export function InfoTip({ term }: { readonly term: GlossaryTerm }) {
  const { m } = useI18n();
  const { title, body } = m.glossary[term];
  const panel = useId();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const note = useRef<HTMLSpanElement>(null);
  const [shift, setShift] = useState(0);

  // Place the panel before it is painted, and again if the window resizes.
  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    const place = () => {
      const anchor = root.current?.getBoundingClientRect().left;
      const width = note.current?.offsetWidth;
      if (anchor !== undefined && width !== undefined) {
        setShift(panelShift(anchor, width, document.documentElement.clientWidth));
      }
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const apply = (event: TipEvent) => {
      const next = nextTip(true, event);
      setOpen(next.open);
      // Only take focus back from inside the tip (or from nowhere), never
      // from whatever else the user is working in.
      const focused = document.activeElement;
      if (next.focusButton && (focused === document.body || root.current?.contains(focused))) {
        button.current?.focus();
      }
    };
    const onPointerDown = (event: PointerEvent) => {
      if (!(event.target instanceof Node && root.current?.contains(event.target))) {
        apply("outside");
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        apply("escape");
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <span ref={root} className="relative inline-flex shrink-0 align-middle">
      <button
        ref={button}
        type="button"
        aria-label={m.whatIs(title)}
        aria-expanded={open}
        aria-controls={panel}
        onClick={() => setOpen(nextTip(open, "toggle").open)}
        className="inline-flex size-5 items-center justify-center rounded-full border border-neutral-400 font-serif text-xs font-normal italic leading-none text-neutral-600 hover:bg-neutral-100 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-neutral-900 aria-expanded:border-neutral-900 aria-expanded:bg-neutral-900 aria-expanded:text-white"
      >
        i
      </button>
      <span
        ref={note}
        id={panel}
        role="note"
        hidden={!open}
        style={{ left: shift }}
        className="absolute top-full z-20 mt-2 block w-[min(24rem,calc(100vw-2rem))] rounded border border-neutral-200 bg-white p-3 text-left text-sm font-normal text-neutral-800 shadow-lg"
      >
        <span className="block font-medium">{title}</span>
        <span className="mt-1 block">{body}</span>
      </span>
    </span>
  );
}
