import { Platform, useColorScheme } from "react-native";

/**
 * Lumen's mobile colours: the desktop app's tokens (apps/web/src/app/styles.css)
 * on a phone's own backgrounds, with one accent, the indigo of the app icon.
 * Text uses the platform's system font (no `fontFamily`).
 */
export interface Colors {
  readonly background: string;
  readonly ink: string;
  readonly ink2: string;
  readonly ink3: string;
  readonly line: string;
  /** A control's track, such as the segmented control's. */
  readonly fill: string;
  /** The selected segment on `fill`. */
  readonly raised: string;
  /** An image cell before its thumbnail arrives. */
  readonly placeholder: string;
  readonly accent: string;
  /** The accent for text on `background`. */
  readonly accentInk: string;
  readonly onAccent: string;
}

const light: Colors = {
  background: "#ffffff",
  ink: "#1d1d1f",
  ink2: "#5f5f66",
  ink3: "#85858c",
  line: "rgba(0, 0, 0, 0.12)",
  fill: "rgba(118, 118, 128, 0.12)",
  raised: "#ffffff",
  placeholder: "#ececf0",
  accent: "#4f46e5",
  accentInk: "#4338ca",
  onAccent: "#ffffff",
};

const dark: Colors = {
  background: "#000000",
  ink: "#f4f4f6",
  ink2: "#ababb3",
  ink3: "#8a8a93",
  line: "rgba(255, 255, 255, 0.15)",
  fill: "rgba(118, 118, 128, 0.24)",
  raised: "#636366",
  placeholder: "#1c1c1f",
  accent: "#5b5bf7",
  accentInk: "#a5a8ff",
  onAccent: "#ffffff",
};

/** The colours for the system's light or dark appearance. */
export function useColors(): Colors {
  return useColorScheme() === "dark" ? dark : light;
}

/** The smallest touch target: 44 pt (Apple HIG) or 48 dp (Material). */
export const TOUCH = Platform.OS === "ios" ? 44 : 48;

/** Type sizes after each platform's own scale (iOS text styles, Material type scale). */
export const type = {
  largeTitle: Platform.select({
    ios: { fontSize: 34, fontWeight: "700" as const },
    default: { fontSize: 28, fontWeight: "400" as const },
  }),
  headline: { fontSize: Platform.OS === "ios" ? 17 : 16, fontWeight: "600" as const },
  body: { fontSize: Platform.OS === "ios" ? 17 : 16 },
  footnote: { fontSize: Platform.OS === "ios" ? 13 : 14 },
  caption: { fontSize: 12 },
};
