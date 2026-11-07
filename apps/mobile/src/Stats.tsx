import { useEffect, useMemo, useRef, useState } from "react";
import { StyleSheet, Switch, Text, View } from "react-native";
import { type Analysis, coreVersion } from "../modules/lumen-core";
import { type FrameStats, frameStats } from "./frameProbe";
import { locale, messages } from "./i18n";
import { TOUCH, type, useColors } from "./theme";
import { space } from "./ui";

/**
 * The spike's measurement panel (docs/research/11-mobile-spike.md): the
 * frame-time probe, the last analysis's timings, the core's version and the
 * 50,000-item test. Its once-a-second updates re-render only this panel, and
 * the probe stops while the panel is hidden.
 */
export function Stats({
  analysis,
  repeat,
  onRepeat,
}: {
  analysis: Analysis | null;
  repeat: boolean;
  onRepeat: (on: boolean) => void;
}) {
  const c = useColors();
  const stats = useFrameStats();
  const version = useMemo(() => coreVersion(), []);
  const line = [type.caption, styles.mono, { color: c.ink2 }];
  return (
    <View style={[styles.panel, { borderColor: c.line }]}>
      <Text style={line}>
        {messages.jsFramesPerSecond} {stats.frames} · {messages.p50} {stats.p50.toFixed(1)} ms ·{" "}
        {messages.p95} {stats.p95.toFixed(1)} ms · {messages.dropped}{" "}
        {(stats.dropped * 100).toFixed(1)}%
      </Text>
      {analysis !== null && (
        <Text style={line}>
          {analysis.analyzed} {messages.analyzed} ·{" "}
          {messages.groupCount(
            new Intl.NumberFormat(locale).format(analysis.groups.length),
            analysis.groups.length,
          )}{" "}
          · {messages.prints} {analysis.printMs} ms · {messages.grouping} {analysis.groupMs} ms
          {analysis.cancelled ? ` · ${messages.cancelled}` : ""}
        </Text>
      )}
      <Text style={line}>
        {messages.core} {version}
      </Text>
      <View style={styles.row}>
        <Text style={[type.footnote, { color: c.ink }]}>
          {messages.repeatTo(new Intl.NumberFormat(locale).format(50_000))}
        </Text>
        <Switch
          value={repeat}
          onValueChange={onRepeat}
          trackColor={{ true: c.accent, false: c.fill }}
          accessibilityLabel={messages.repeatAccessibilityLabel}
        />
      </View>
    </View>
  );
}

/** Frame intervals on the JS thread, summarized every second. */
function useFrameStats(): FrameStats {
  const [stats, setStats] = useState<FrameStats>(frameStats([]));
  const intervals = useRef<number[]>([]);
  useEffect(() => {
    let last = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      intervals.current.push(now - last);
      last = now;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    const report = setInterval(() => {
      setStats(frameStats(intervals.current));
      intervals.current = [];
    }, 1000);
    return () => {
      cancelAnimationFrame(frame);
      clearInterval(report);
    };
  }, []);
  return stats;
}

const styles = StyleSheet.create({
  panel: {
    paddingHorizontal: space.edge,
    paddingTop: space.gap / 2,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  mono: { fontVariant: ["tabular-nums"] },
  row: {
    minHeight: TOUCH,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
  },
});
