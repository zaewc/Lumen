const KEY = "lumen.update.dismissed";

/** The last version whose prominent notice the user declined, across launches. */
export function dismissedUpdate(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function storeDismissedUpdate(version: string): void {
  try {
    localStorage.setItem(KEY, version);
  } catch {
    // The component still remembers the choice for this session.
  }
}
