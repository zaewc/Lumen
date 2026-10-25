import type { ReactNode } from "react";
import { Platform, Pressable, StyleSheet, Text, View, type ViewStyle } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { type Colors, TOUCH, type, useColors } from "./theme";

/**
 * The screen inside the system bars, notch, home indicator and gesture bar,
 * with each platform's real insets (react-native-safe-area-context; needs the
 * `SafeAreaProvider` in `App`).
 */
export function SafeArea({ children, style }: { children: ReactNode; style: ViewStyle }) {
  return (
    <SafeAreaView edges={["top", "bottom", "left", "right"]} style={[styles.fill, style]}>
      {children}
    </SafeAreaView>
  );
}

/** The screen's one primary action: full width, in thumb reach at the bottom. */
export function Button({ label, onPress }: { label: string; onPress: () => void }) {
  const c = useColors();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      android_ripple={{ color: "rgba(255, 255, 255, 0.24)" }}
      style={({ pressed }) => [
        styles.button,
        { backgroundColor: c.accent, opacity: pressed && Platform.OS === "ios" ? 0.7 : 1 },
      ]}
    >
      <Text style={[type.headline, { color: c.onAccent }]}>{label}</Text>
    </Pressable>
  );
}

/** A secondary action: accent text with a full-size touch target. */
export function TextButton({ label, onPress }: { label: string; onPress: () => void }) {
  const c = useColors();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      android_ripple={{ color: c.fill, borderless: true }}
      style={({ pressed }) => [styles.textButton, { opacity: pressed ? 0.5 : 1 }]}
    >
      <Text style={[type.body, { color: c.accentInk }]}>{label}</Text>
    </Pressable>
  );
}

/** A two-way choice, as the platforms' segmented controls show it. */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  const c = useColors();
  return (
    <View accessibilityRole="tablist" style={[styles.segmented, { backgroundColor: c.fill }]}>
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <Pressable
            key={option.value}
            accessibilityRole="tab"
            accessibilityState={{ selected }}
            onPress={() => onChange(option.value)}
            style={[styles.segment, selected && { backgroundColor: c.raised }]}
          >
            <Text
              numberOfLines={1}
              style={[type.footnote, { color: c.ink, fontWeight: selected ? "600" : "400" }]}
            >
              {option.label}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

/** A whole-screen state: what happened and, through `action`, what to do next. */
export function Message({
  title,
  body,
  action,
  busy,
}: {
  title: string;
  body?: string;
  action?: ReactNode;
  busy?: ReactNode;
}) {
  const c = useColors();
  return (
    <View style={styles.fill}>
      <View style={styles.message}>
        {busy}
        <Text style={[type.headline, styles.center, { color: c.ink }]}>{title}</Text>
        {body !== undefined && (
          <Text style={[type.body, styles.center, { color: c.ink2 }]}>{body}</Text>
        )}
      </View>
      {action !== undefined && <View style={styles.bottom}>{action}</View>}
    </View>
  );
}

/** Spacing shared by the screens. */
export const space = { edge: 16, gap: 8 };

export function hairline(c: Colors): ViewStyle {
  return { borderColor: c.line, borderTopWidth: StyleSheet.hairlineWidth };
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  button: {
    minHeight: Math.max(TOUCH, 50),
    alignItems: "center",
    justifyContent: "center",
    borderRadius: Platform.OS === "ios" ? 12 : 25,
    overflow: "hidden",
  },
  textButton: {
    minHeight: TOUCH,
    minWidth: TOUCH,
    paddingHorizontal: space.gap,
    alignItems: "center",
    justifyContent: "center",
  },
  segmented: { flexDirection: "row", minHeight: TOUCH, padding: 3, borderRadius: 10 },
  segment: { flex: 1, alignItems: "center", justifyContent: "center", borderRadius: 8 },
  message: { flex: 1, alignItems: "center", justifyContent: "center", gap: space.gap, padding: 32 },
  center: { textAlign: "center" },
  bottom: { padding: space.edge },
});
