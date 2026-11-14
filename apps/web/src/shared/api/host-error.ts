import { errorMessage } from "@/shared/lib";

/** A command the desktop host refused or failed, carrying the host's own message. */
export class HostError extends Error {
  override name = "HostError";
}

/**
 * A Tauri `invoke` rejection as an `Error`. The host rejects with its
 * serialized `CommandError` (`{ message }`), and Tauri itself with a string;
 * both become the message. Errors pass through unchanged. Anything else keeps
 * no message (the UI then shows a generic line) and is kept as `cause`.
 */
export function hostError(rejection: unknown): Error {
  if (rejection instanceof Error) {
    return rejection;
  }
  return new HostError(errorMessage(rejection) ?? "", { cause: rejection });
}

/**
 * The host's answer when the person closed its confirmation window without
 * confirming (FS-1): nothing changed, and it is not a failure to show.
 */
const DECLINED = "not confirmed; nothing was changed";

/** The person declined in the host's confirmation window. */
export function declined(error: unknown): boolean {
  return error instanceof HostError && error.message === DECLINED;
}
