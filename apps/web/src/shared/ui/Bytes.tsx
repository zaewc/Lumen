import { formatBytes } from "@/shared/lib";

/**
 * A size set as a figure: the number, then its unit at 0.8 em in `ink-2`, so
 * a column of sizes reads as a scale (docs/design/visual-language.md, type).
 * Sizes inside sentences stay plain text.
 */
export function Bytes({ value }: { /** A decimal byte count. */ readonly value: string }) {
  const text = formatBytes(value);
  const space = text.lastIndexOf(" ");
  return (
    <>
      {text.slice(0, space)}{" "}
      <span className="text-[0.8em] font-normal text-ink-2">{text.slice(space + 1)}</span>
    </>
  );
}
