import { z } from "zod";

/** The desktop host's serialized `CommandError` (an untrusted edge). */
const zHostFailure = z.object({ message: z.string() });

/**
 * The words an error carries, for "Details": an `Error`'s message, a plain
 * string, or a host failure's `message`. Anything else (or an empty text)
 * has none, so the caller shows a generic line instead of `[object Object]`.
 */
export function errorMessage(error: unknown): string | null {
  let text: string | null = null;
  if (error instanceof Error) {
    text = error.message;
  } else if (typeof error === "string") {
    text = error;
  } else {
    const failure = zHostFailure.safeParse(error);
    text = failure.success ? failure.data.message : null;
  }
  const trimmed = text?.trim() ?? "";
  return trimmed === "" ? null : trimmed;
}
