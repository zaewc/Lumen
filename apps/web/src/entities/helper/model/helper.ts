import type { HelperView } from "@lumen/api-client";

/** What the helper's Settings row says (ADR-0063). */
export type HelperLine =
  | { readonly kind: "off" }
  | { readonly kind: "waiting" }
  | { readonly kind: "on"; readonly version: string }
  | {
      readonly kind: "problem";
      readonly problem: "outdated" | "unsigned" | "no_answer" | "not_found" | "mismatch" | "failed";
    };

/** What the row offers. */
export type HelperAction = "enable" | "disable" | "open_login_items";

/** The row's line, or `null` where there is no helper (off macOS). */
export function helperLine(view: HelperView): HelperLine | null {
  switch (view.registration) {
    case "unsupported":
      return null;
    case "not_registered":
      return { kind: "off" };
    case "requires_approval":
      return { kind: "waiting" };
    case "not_found":
      return { kind: "problem", problem: "not_found" };
    case "enabled":
      break;
  }
  const reach = view.reach;
  switch (reach.kind) {
    case "answered":
      return reach.current
        ? { kind: "on", version: reach.version }
        : { kind: "problem", problem: "outdated" };
    case "unsigned":
      return { kind: "problem", problem: "unsigned" };
    case "unavailable":
    case "not_asked":
      return { kind: "problem", problem: "no_answer" };
    case "not_signed_as_required":
      return { kind: "problem", problem: "mismatch" };
    case "refused":
    case "failed":
      return { kind: "problem", problem: "failed" };
  }
}

/** The buttons for a registration, most useful first. */
export function helperActions(view: HelperView): readonly HelperAction[] {
  switch (view.registration) {
    case "unsupported":
      return [];
    case "not_registered":
    case "not_found":
      return ["enable"];
    case "requires_approval":
      return ["open_login_items", "disable"];
    case "enabled":
      return ["disable"];
  }
}
