// Static README links styled after the desktop Segmented control. No runtime JS.
// Regenerate after changing locale.ts, Segmented.tsx or the app colour tokens:
//   node scripts/generate-readme-language-tabs.mjs
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const read = (path) => readFile(new URL(path, root), "utf8");
const locales = await read("apps/web/src/shared/i18n/locale.ts");
const styles = await read("apps/web/src/app/styles.css");
const order = locales.match(/export const LOCALES = \[(.*?)\] as const;/)?.[1];
assert.ok(order, "Review the locale declaration before generating tabs");
const names = [...locales.matchAll(/^  (?:"([\w-]+)"|(\w+)): "([^"]+)",$/gm)];
const languages = [...order.matchAll(/"([\w-]+)"/g)].map(([, locale]) => {
  const name = names.find(([, quoted, bare]) => (quoted ?? bare) === locale)?.[3];
  assert.ok(name, `Missing language name: ${locale}`);
  return { locale, name, file: locale === "en" ? "README.md" : `README.${locale}.md` };
});
const fonts = styles.match(/--font-sans:([^;]+);/)?.[1].trim().replace(/\s+/g, " ");
assert.ok(fonts, "Missing app system font stack");
const escape = (value) => value.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");
const folder = "docs/assets/readme/language-tabs";
await mkdir(new URL(folder, root), { recursive: true });

for (const { locale, name } of languages) {
  // Keep Segmented.tsx's 13px / 500 type, with a 44px target for README links.
  // A consistent width fits every current label and wraps as ordinary links.
  for (const theme of ["light", "dark"]) {
    const token = (name) => {
      const values = [...styles.matchAll(new RegExp(`--lm-${name}: ([^;]+);`, "g"))];
      assert.equal(values.length, 2, `Expected light and dark tokens: ${name}`);
      return values[theme === "dark" ? 1 : 0][1];
    };
    for (const selected of [false, true]) {
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="88" height="44" viewBox="0 0 88 44">
  <title>${escape(name)}</title>
  <path d="M0 43.5H88" stroke="${token("line")}"/>
  <text x="44" y="27" text-anchor="middle" fill="${token(selected ? "ink" : "ink-2")}" font-family="${escape(fonts)}" font-size="13" font-weight="500">${escape(name)}</text>
${selected ? `  <path d="M0 43H88" stroke="${token("accent")}" stroke-width="2"/>\n` : ""}</svg>
`;
      await writeFile(new URL(`${folder}/${locale}${selected ? "-selected" : ""}-${theme}.svg`, root), svg);
    }
  }
}

for (const current of languages) {
  const source = await read(current.file);
  const links = languages.map(({ locale, name, file }) => {
    const selected = locale === current.locale;
    const asset = `${folder}/${locale}${selected ? "-selected" : ""}`;
    return `<a href="${file}" lang="${locale}" hreflang="${locale}"><picture><source media="(prefers-color-scheme: dark)" srcset="${asset}-dark.svg"><img src="${asset}-light.svg" width="88" height="44" alt="${name}${selected ? " ✓" : ""}"></picture></a>`;
  });
  // Adjacent links keep the baseline continuous; replaced images still wrap.
  const block = `<!-- language-tabs:start -->\n<p>\n${links.join("")}\n</p>\n<!-- language-tabs:end -->`;
  const pattern = /<!-- language-tabs:start -->[\s\S]*?<!-- language-tabs:end -->/;
  assert.ok(pattern.test(source), `Missing language navigation markers: ${current.file}`);
  await writeFile(new URL(current.file, root), source.replace(pattern, block));
}
