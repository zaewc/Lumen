import { StatusBar } from "expo-status-bar";
import { useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { coreVersion } from "../modules/lumen-core";
import { ReviewGrid } from "./ReviewGrid";

/**
 * The mobile app's shell (ADR-0007). JavaScript only ever receives paged,
 * read-only view models and sends intents; scanning and decisions stay in
 * native code and the Rust core.
 */
export function App() {
  const [grid, setGrid] = useState(false);
  if (grid) {
    return <ReviewGrid onClose={() => setGrid(false)} />;
  }
  return (
    <View style={styles.screen}>
      <Text style={styles.title}>Lumen</Text>
      <Text style={styles.body}>
        Storage cleanup that keeps everything reversible until you decide otherwise.
      </Text>
      <Text style={styles.version}>Core {coreVersion()}</Text>
      <Pressable onPress={() => setGrid(true)} style={styles.button}>
        <Text style={styles.buttonText}>Open the review grid (spike)</Text>
      </Pressable>
      <StatusBar style="auto" />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, alignItems: "center", justifyContent: "center", padding: 24 },
  title: { fontSize: 28, fontWeight: "600" },
  body: { marginTop: 8, textAlign: "center", color: "#525252" },
  version: { marginTop: 24, fontSize: 12, color: "#737373" },
  button: { marginTop: 24, paddingHorizontal: 16, paddingVertical: 8, backgroundColor: "#171717" },
  buttonText: { color: "#ffffff" },
});
