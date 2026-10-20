/** The folders typed into the form, one per line, blank lines ignored. */
export function rootsOf(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}
