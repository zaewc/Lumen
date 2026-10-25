import { StatusBar } from "expo-status-bar";
import { ReviewGrid } from "./ReviewGrid";
import { useColors } from "./theme";
import { SafeArea } from "./ui";

/**
 * The mobile app's shell (ADR-0007). JavaScript only ever receives paged,
 * read-only view models and sends intents; scanning and decisions stay in
 * native code and the Rust core.
 */
export function App() {
  const c = useColors();
  return (
    <SafeArea style={{ backgroundColor: c.background }}>
      <ReviewGrid />
      <StatusBar style="auto" />
    </SafeArea>
  );
}
