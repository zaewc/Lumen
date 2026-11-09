import type { TextStyle } from "react-native";

export const suitFonts = {
  "SUIT-Thin": require("../../../fonts/SUIT-Thin.otf"),
  "SUIT-ExtraLight": require("../../../fonts/SUIT-ExtraLight.otf"),
  "SUIT-Light": require("../../../fonts/SUIT-Light.otf"),
  "SUIT-Regular": require("../../../fonts/SUIT-Regular.otf"),
  "SUIT-Medium": require("../../../fonts/SUIT-Medium.otf"),
  "SUIT-SemiBold": require("../../../fonts/SUIT-SemiBold.otf"),
  "SUIT-Bold": require("../../../fonts/SUIT-Bold.otf"),
  "SUIT-ExtraBold": require("../../../fonts/SUIT-ExtraBold.otf"),
  "SUIT-Heavy": require("../../../fonts/SUIT-Heavy.otf"),
} as const;

export type SuitWeight = 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900;

const familyByWeight: Record<SuitWeight, keyof typeof suitFonts> = {
  100: "SUIT-Thin",
  200: "SUIT-ExtraLight",
  300: "SUIT-Light",
  400: "SUIT-Regular",
  500: "SUIT-Medium",
  600: "SUIT-SemiBold",
  700: "SUIT-Bold",
  800: "SUIT-ExtraBold",
  900: "SUIT-Heavy",
};

export function suitFont(weight: SuitWeight): TextStyle {
  return { fontFamily: familyByWeight[weight] };
}
