import { StatusBar } from "expo-status-bar";
import { StyleSheet, Text, View } from "react-native";

/**
 * The mobile app's shell (ADR-0007). JavaScript only ever receives paged,
 * read-only view models and sends intents; scanning and decisions stay in
 * native code and the Rust core.
 */
export function App() {
  return (
    <View style={styles.screen}>
      <Text style={styles.title}>Lumen</Text>
      <Text style={styles.body}>
        Storage cleanup that keeps everything reversible until you decide otherwise.
      </Text>
      <StatusBar style="auto" />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, alignItems: "center", justifyContent: "center", padding: 24 },
  title: { fontSize: 28, fontWeight: "600" },
  body: { marginTop: 8, textAlign: "center", color: "#525252" },
});
