import { requireNativeViewManager } from "expo-modules-core";
import type { ReactNode } from "react";
import { Platform, StyleSheet, View, type ViewProps } from "react-native";
import { material, useColors } from "./theme";

// Android keeps an opaque, tinted surface; no per-frame blur or new dependency.
const NativeMaterial =
  Platform.OS === "ios" ? requireNativeViewManager<ViewProps>("LumenMaterial") : null;

/** Integrated chrome, with stable text above a native adaptive backdrop. */
export function Material({ children, style, ...props }: ViewProps & { children: ReactNode }) {
  const c = useColors();
  return (
    <View {...props} style={[{ backgroundColor: c.chrome }, style]}>
      {NativeMaterial !== null && (
        <NativeMaterial pointerEvents="none" accessible={false} style={StyleSheet.absoluteFill} />
      )}
      <View
        pointerEvents="none"
        style={[
          StyleSheet.absoluteFill,
          { backgroundColor: c.tint, borderTopColor: c.highlight, borderTopWidth: material.edge },
        ]}
      />
      {children}
    </View>
  );
}
