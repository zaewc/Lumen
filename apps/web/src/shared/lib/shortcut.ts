import { useEffect, useRef } from "react";
import { isMac, userAgent } from "./platform";

/** The parts of a key press a shortcut looks at (a `KeyboardEvent` has them all). */
export interface KeyPress {
  readonly key: string;
  /** The physical key, which a Korean (or other) input source does not change. */
  readonly code: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/** The physical key that types `key` (a letter or digit) on a US layout: "KeyR", "Digit1". */
function codeOf(key: string): string {
  return /^\d$/.test(key) ? `Digit${key}` : `Key${key.toUpperCase()}`;
}

/**
 * Whether `press` is the platform's command shortcut for `key` (a letter or
 * digit): ⌘ and the key on a Mac, Ctrl and the key elsewhere, with no other
 * modifier. Letters match in either case, and by their physical key too,
 * so ⌘R works while typing Korean (where the key gives "ㄱ").
 */
export function isCommand(press: KeyPress, key: string, mac: boolean): boolean {
  const modifier = mac ? press.metaKey && !press.ctrlKey : press.ctrlKey && !press.metaKey;
  const same = press.key.toLowerCase() === key.toLowerCase() || press.code === codeOf(key);
  return modifier && !press.altKey && !press.shiftKey && same;
}

/** How the shortcut is written for people: "⌘R" on a Mac, "Ctrl+R" elsewhere. */
export function shortcutLabel(key: string, mac: boolean): string {
  return mac ? `⌘${key.toUpperCase()}` : `Ctrl+${key.toUpperCase()}`;
}

/** The shortcut for `aria-keyshortcuts`: "Meta+R" on a Mac, "Control+R" elsewhere. */
export function ariaShortcut(key: string, mac: boolean): string {
  return `${mac ? "Meta" : "Control"}+${key.toUpperCase()}`;
}

/** What a control shows and announces for its command shortcut. */
export function shortcutProps(
  label: string,
  key: string,
): { readonly title: string; readonly "aria-keyshortcuts": string } {
  const mac = isMac(userAgent());
  return {
    title: `${label} (${shortcutLabel(key, mac)})`,
    "aria-keyshortcuts": ariaShortcut(key, mac),
  };
}

/**
 * Runs `action` on the platform's command shortcut for `key` while
 * `enabled`, and keeps the browser from acting on it too (⌘R would reload).
 * It stays quiet while a modal dialog is open, since the dialog has the
 * user's attention, and for a held-down key.
 */
export function useShortcut(key: string, action: () => void, enabled = true): void {
  const latest = useRef(action);
  latest.current = action;
  useEffect(() => {
    if (!enabled) {
      return;
    }
    const mac = isMac(userAgent());
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.repeat ||
        !isCommand(event, key, mac) ||
        document.querySelector("dialog[open]") !== null
      ) {
        return;
      }
      event.preventDefault();
      latest.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [key, enabled]);
}
