export const THEMES = ["system", "light", "dark"] as const;
export type Theme = (typeof THEMES)[number];
const KEY = "lumen.theme";

export function storedTheme(): Theme {
  try {
    const value = localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

export function storeTheme(theme: Theme): void {
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // The choice still applies for this session when storage is unavailable.
  }
}

export function systemIsDark(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches;
}

export function followSystem(changed: () => void): () => void {
  if (typeof matchMedia !== "function") return () => {};
  const media = matchMedia("(prefers-color-scheme: dark)");
  media.addEventListener("change", changed);
  return () => media.removeEventListener("change", changed);
}
