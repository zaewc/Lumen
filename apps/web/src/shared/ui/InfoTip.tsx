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
        className="inline-flex size-[18px] items-center justify-center rounded-full border border-line-strong font-serif text-[11px] font-normal italic leading-none text-ink-3 hover:border-ink-3 hover:text-ink-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent aria-expanded:border-accent aria-expanded:bg-accent aria-expanded:text-white"
      >
        i
      </button>
      <span
        ref={note}
        id={panel}
        role="note"
        hidden={!open}
        style={{ left: shift }}
        className="absolute top-full z-20 mt-2 block w-[min(22rem,calc(100vw-2rem))] rounded-xl border border-line bg-raised p-3.5 text-left text-[13px] leading-relaxed font-normal tracking-normal text-ink-2 normal-case shadow-xl shadow-black/10 dark:shadow-black/40"
      >
        <span className="block font-semibold text-ink">{title}</span>
        <span className="mt-1 block">{body}</span>
      </span>
    </span>
  );
}
