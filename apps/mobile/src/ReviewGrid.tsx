import { FlashList } from "@shopify/flash-list";
import { Image } from "expo-image";
import { useEffect, useMemo, useRef, useState } from "react";
import { Pressable, StyleSheet, Text, useWindowDimensions, View } from "react-native";
import { type Access, count, type Photo, page, requestAccess } from "../modules/lumen-photos";
import { type FrameStats, frameStats } from "./frameProbe";

const COLUMNS = 4;
const PAGE = 500;
const TARGET = 50_000;

/** The performance spike's review grid (ADR-0007, roadmap 11.16b). */
export function ReviewGrid({ onClose }: { readonly onClose: () => void }) {
  const [access, setAccess] = useState<Access | null>(null);
  const [photos, setPhotos] = useState<Photo[]>([]);
  const [repeat, setRepeat] = useState(false);
  const [stats, setStats] = useState<FrameStats>(frameStats([]));
  const { width } = useWindowDimensions();
  const side = Math.floor(width / COLUMNS);

  useEffect(() => {
    void requestAccess().then((granted) => {
      setAccess(granted);
      if (granted === "granted" || granted === "limited") {
        const total = count();
        const all: Photo[] = [];
        for (let offset = 0; offset < total; offset += PAGE) {
          all.push(...page(offset, PAGE));
        }
        setPhotos(all);
      }
    });
  }, []);

  // The library itself, or its own photos cycled to 50k items to load the
  // grid like a large library without adding anything to it.
  const items = useMemo(() => {
    if (!repeat || photos.length === 0) {
      return photos.map((photo, index) => ({ key: `${index}`, photo }));
    }
    const cycled: { key: string; photo: Photo }[] = [];
    for (let index = 0; index < TARGET; index += 1) {
      const photo = photos[index % photos.length];
      if (photo !== undefined) {
        cycled.push({ key: `${index}`, photo });
      }
    }
    return cycled;
  }, [photos, repeat]);

  // Frame intervals on the JS thread, reported every second.
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

  return (
    <View style={styles.screen}>
      <View style={styles.bar}>
        <Pressable onPress={onClose}>
          <Text style={styles.link}>Close</Text>
        </Pressable>
        <Text>
          {items.length} items · access {access ?? "…"}
        </Text>
        <Pressable onPress={() => setRepeat((on) => !on)}>
          <Text style={styles.link}>{repeat ? "Library only" : "Repeat to 50k"}</Text>
        </Pressable>
      </View>
      <Text style={styles.probe}>
        JS frames/s {stats.frames} · p50 {stats.p50.toFixed(1)} ms · p95 {stats.p95.toFixed(1)} ms ·
        dropped {(stats.dropped * 100).toFixed(1)}%
      </Text>
      <FlashList
        data={items}
        numColumns={COLUMNS}
        keyExtractor={(item) => item.key}
        renderItem={({ item }) => (
          <Image
            source={{ uri: item.photo.uri }}
            recyclingKey={item.key}
            style={{ width: side, height: side }}
            contentFit="cover"
            cachePolicy="memory"
          />
        )}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, paddingTop: 48 },
  bar: { flexDirection: "row", justifyContent: "space-between", paddingHorizontal: 12 },
  link: { color: "#1d4ed8" },
  probe: { paddingHorizontal: 12, paddingVertical: 6, fontSize: 12, fontVariant: ["tabular-nums"] },
});
