import { Platform, StyleSheet, useColorScheme } from "react-native";

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
  readonly chrome: string;
  readonly tint: string;
  readonly highlight: string;
  readonly shadow: string;
  readonly selected: string;
}

const light: Colors = {
  background: "#f7f9fc",
  ink: "#182232",
  ink2: "#4e5d71",
  ink3: "#66758a",
  line: "rgba(65, 85, 117, 0.12)",
  fill: "rgba(65, 85, 117, 0.07)",
  raised: "#ffffff",
  placeholder: "#ececf0",
  accent: "#435bd6",
  accentInk: "#344fbd",
  onAccent: "#ffffff",
  chrome: "#f0f1f3",
  tint: "rgba(255, 255, 255, 0.015)",
  highlight: "rgba(255, 255, 255, 0.72)",
  shadow: "rgba(24, 39, 69, 0.12)",
  selected: "#ffffff",
};

const dark: Colors = {
  background: "#121821",
  ink: "#ecf1f8",
  ink2: "#b0bfd1",
  ink3: "#91a3ba",
  line: "rgba(172, 195, 230, 0.13)",
  fill: "rgba(118, 118, 128, 0.24)",
  raised: "#222d3d",
  placeholder: "#1c1c1f",
  accent: "#5268d1",
  accentInk: "#a4b4ff",
  onAccent: "#ffffff",
  chrome: "#202328",
  tint: "rgba(255, 255, 255, 0.015)",
  highlight: "rgba(212, 228, 255, 0.1)",
  shadow: "rgba(0, 4, 14, 0.28)",
  selected: "#3d4147",
};

/** The colours for the system's light or dark appearance. */
export function useColors(): Colors {
  return useColorScheme() === "dark" ? dark : light;
}

/** The smallest touch target: 44 pt (Apple HIG) or 48 dp (Material). */
export const TOUCH = Platform.OS === "ios" ? 44 : 48;

/** Shared material geometry; structural chrome stays square and integrated. */
export const material = {
  edge: StyleSheet.hairlineWidth,
  controlRadius: 10,
  segmentRadius: 7,
};

/** Small controls alone receive elevation; never the photo grid or its cells. */
export function controlMaterial(c: Colors) {
  return {
    borderTopWidth: material.edge,
    borderTopColor: c.highlight,
    boxShadow: [{ offsetX: 0, offsetY: 2, blurRadius: 5, color: c.shadow }],
  };
}

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
