// Paths arrive as the core reported them: Unix ("/Users/ana/Library") or
// Windows ("C:\Users\ana\AppData"). They are only ever shown, never used to
// act on anything (mutations name plans and operations, ADR-0009).

/**
 * The last component of a path, for a short, friendly title: "DerivedData"
 * for "/Users/ana/Library/Developer/Xcode/DerivedData/". Both separators are
 * understood. A root ("/", "C:\") has no last component and is shown whole.
 */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter((part) => part.length > 0);
  const last = parts.at(-1);
  if (last === undefined) {
    return path;
  }
  return parts.length === 1 && /^[A-Za-z]:$/.test(last) ? path : last;
}
