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

/**
 * Everything before the last component, for a row's subtitle:
 * "/Users/ana/Library/Caches" for "/Users/ana/Library/Caches/App". A path
 * without a parent gives "".
 */
export function parentPath(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (cut < 0) {
    return "";
  }
  return cut === 0 ? trimmed.slice(0, 1) : trimmed.slice(0, cut);
}
