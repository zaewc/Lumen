import { useFonts } from "expo-font";
import { StatusBar } from "expo-status-bar";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { suitFonts } from "./fonts";
import { ReviewGrid } from "./ReviewGrid";
import { useColors } from "./theme";
import { SafeArea } from "./ui";

/**
 * The mobile app's shell (ADR-0007). JavaScript only ever receives paged,
 * read-only view models and sends intents; scanning and decisions stay in
 * native code and the Rust core.
 */
export function App() {
  const [fontsLoaded, fontError] = useFonts(suitFonts);
  const c = useColors();

  if (fontError) {
    throw fontError;
  }
  if (!fontsLoaded) {
    return null;
  }

  return (
    <SafeAreaProvider>
      <SafeArea style={{ backgroundColor: c.background }}>
        <ReviewGrid />
        <StatusBar style="auto" />
      </SafeArea>
    </SafeAreaProvider>
  );
}
