import { FlashList } from "@shopify/flash-list";
import { Image } from "expo-image";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  AppState,
  Linking,
  Platform,
  StyleSheet,
  Text,
  useWindowDimensions,
  View,
} from "react-native";
import { type Analysis, analyze } from "../modules/lumen-core";
import { count, type Photo, page, requestAccess } from "../modules/lumen-photos";
import { Stats } from "./Stats";
import { type, useColors } from "./theme";
import { Button, hairline, Message, Segmented, space, TextButton } from "./ui";

const COLUMNS = 4;
/** Space between thumbnails, in points. */
const GAP = 1;
const PAGE = 500;
const TARGET = 50_000;
const ANALYZE = 5_000;
// Euclidean thresholds: Vision feature prints (iOS), or the square root of a
// 64-bit hash's Hamming distance of 10 (Android). To be tuned on a labelled set.
const THRESHOLD = Platform.OS === "ios" ? 0.6 : Math.sqrt(10);

type Library =
  | { readonly state: "loading" }
  | { readonly state: "denied" }
  | { readonly state: "failed"; readonly message: string }
  | {
      readonly state: "ready";
      readonly limited: boolean;
      readonly photos: readonly Photo[];
    };

type Similar =
  | { readonly state: "idle" }
  | { readonly state: "running" }
  | { readonly state: "failed"; readonly message: string }
  | { readonly state: "done"; readonly analysis: Analysis };

/** A grid cell: a photo, or a full-width row that starts a group. */
type Item =
  | { readonly kind: "photo"; readonly key: string; readonly photo: Photo }
  | { readonly kind: "group"; readonly key: string; readonly size: number };

type Show = "all" | "similar";

const SHOW = [
  { value: "all", label: "All" },
  { value: "similar", label: "Similar" },
] as const;

const NONE: readonly Photo[] = [];
const number = new Intl.NumberFormat("en");
const photos = (n: number) => `${number.format(n)} ${n === 1 ? "photo" : "photos"}`;
const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Asks for access, then lists the library, newest first, in native pages. */
async function loadLibrary(): Promise<Library> {
  try {
    const access = await requestAccess();
    if (access !== "granted" && access !== "limited") {
      return { state: "denied" };
    }
    // Let the loading state paint before the synchronous enumeration.
    await new Promise((resolve) => setTimeout(resolve, 0));
    const total = count();
    const all: Photo[] = [];
    for (let offset = 0; offset < total; offset += PAGE) {
      all.push(...page(offset, PAGE));
    }
    return { state: "ready", limited: access === "limited", photos: all };
  } catch (error) {
    return { state: "failed", message: messageOf(error) };
  }
}

/** Each group's members together, each group starting a new row under its size. */
function grouped(library: readonly Photo[], analysis: Analysis): Item[] {
  const byId = new Map(library.map((photo) => [photo.id, photo]));
  const items: Item[] = [];
  analysis.groups.forEach((group, g) => {
    const members = group.flatMap((id) => byId.get(id) ?? []);
    if (members.length > 0) {
      items.push({ kind: "group", key: `g${g}`, size: members.length });
      for (const photo of members) {
        items.push({ kind: "photo", key: `${g}:${photo.id}`, photo });
      }
    }
  });
  return items;
}

/**
 * The photo review grid (ADR-0007, roadmap 11.16b/c): the library in a dense
 * grid, near-duplicate groups on request, and the spike's measurement panel
 * behind "Stats". The primary action sits at the bottom, in thumb reach.
 */
export function ReviewGrid() {
  const c = useColors();
  const { width } = useWindowDimensions();
  const [library, setLibrary] = useState<Library>({ state: "loading" });
  const [similar, setSimilar] = useState<Similar>({ state: "idle" });
  const [show, setShow] = useState<Show>("all");
  const [repeat, setRepeat] = useState(false);
  const [stats, setStats] = useState(false);

  const load = useCallback(() => {
    setLibrary({ state: "loading" });
    void loadLibrary().then(setLibrary);
  }, []);
  useEffect(load, [load]);

  // Look again when the person comes back, from Settings or with new photos.
  const recheck =
    library.state === "denied" || (library.state === "ready" && library.photos.length === 0);
  useEffect(() => {
    if (!recheck) {
      return;
    }
    const subscription = AppState.addEventListener("change", (state) => {
      if (state === "active") {
        load();
      }
    });
    return () => subscription.remove();
  }, [recheck, load]);

  const findSimilar = () => {
    setSimilar({ state: "running" });
    setShow("all");
    analyze(ANALYZE, THRESHOLD).then(
      (analysis) => {
        setSimilar({ state: "done", analysis });
        setShow(analysis.groups.length > 0 ? "similar" : "all");
      },
      (error: unknown) => setSimilar({ state: "failed", message: messageOf(error) }),
    );
  };

  const all = library.state === "ready" ? library.photos : NONE;
  const analysis = similar.state === "done" ? similar.analysis : null;
  const grouping = show === "similar" && analysis !== null;

  // The library itself, or its own photos cycled to 50k items to load the
  // grid like a large library without adding anything to it.
  const items = useMemo((): Item[] => {
    if (grouping) {
      return grouped(all, analysis);
    }
    const length = repeat && all.length > 0 ? TARGET : all.length;
    const cells: Item[] = [];
    for (let index = 0; index < length; index += 1) {
      const photo = all[index % all.length];
      if (photo !== undefined) {
        cells.push({ kind: "photo", key: `${index}`, photo });
      }
    }
    return cells;
  }, [all, analysis, grouping, repeat]);

  const cell = width / COLUMNS;
  const image = useMemo(
    () => ({
      width: cell - GAP,
      height: cell - GAP,
      margin: GAP / 2,
      backgroundColor: c.placeholder,
    }),
    [cell, c.placeholder],
  );
  const renderItem = useCallback(
    ({ item }: { item: Item }) => {
      switch (item.kind) {
        case "photo":
          return (
            <Image
              source={{ uri: item.photo.uri }}
              recyclingKey={item.key}
              style={image}
              contentFit="cover"
              cachePolicy="memory"
            />
          );
        case "group":
          return (
            <Text style={[type.footnote, styles.group, { color: c.ink2 }]}>
              {number.format(item.size)} similar
            </Text>
          );
      }
    },
    [image, c.ink2],
  );

  const header = (subtitle?: string, action?: ReactNode) => (
    <View style={styles.header}>
      <View style={styles.fill}>
        <Text accessibilityRole="header" style={[type.largeTitle, { color: c.ink }]}>
          Photos
        </Text>
        {subtitle !== undefined && (
          <Text style={[type.footnote, { color: c.ink2 }]}>{subtitle}</Text>
        )}
      </View>
      {action}
    </View>
  );

  switch (library.state) {
    case "loading":
      return (
        <View style={styles.fill}>
          {header()}
          <Message title="Loading photos…" busy={<ActivityIndicator color={c.ink3} />} />
        </View>
      );
    case "denied":
      return (
        <View style={styles.fill}>
          {header()}
          <Message
            title="No access to photos"
            body="Allow Lumen to read your photo library in Settings. Nothing is changed without your confirmation."
            action={<Button label="Open Settings" onPress={() => void Linking.openSettings()} />}
          />
        </View>
      );
    case "failed":
      return (
        <View style={styles.fill}>
          {header()}
          <Message
            title="Photos could not be loaded"
            body={library.message}
            action={<Button label="Try again" onPress={load} />}
          />
        </View>
      );
    case "ready":
      break;
  }

  if (library.photos.length === 0) {
    return (
      <View style={styles.fill}>
        {header()}
        <Message title="No photos" body="Photos you take or save show up here." />
      </View>
    );
  }

  let subtitle = `${photos(all.length)}${library.limited ? " you selected" : ""}`;
  if (grouping) {
    const members = items.length - analysis.groups.length;
    subtitle = `${photos(members)} in ${number.format(analysis.groups.length)} groups`;
  } else if (repeat) {
    subtitle = `${number.format(items.length)} items, repeated from ${photos(all.length)}`;
  }

  return (
    <View style={styles.fill}>
      {header(
        subtitle,
        <TextButton label={stats ? "Hide stats" : "Stats"} onPress={() => setStats((on) => !on)} />,
      )}
      {stats && <Stats analysis={analysis} repeat={repeat} onRepeat={setRepeat} />}
      <View style={styles.fill}>
        <FlashList
          key={show}
          data={items}
          numColumns={COLUMNS}
          keyExtractor={(item) => item.key}
          getItemType={(item) => item.kind}
          overrideItemLayout={(layout, item) => {
            if (item.kind === "group") {
              layout.span = COLUMNS;
            }
          }}
          renderItem={renderItem}
        />
      </View>
      <View style={[styles.bar, hairline(c)]}>
        {similar.state === "idle" && <Button label="Find similar photos" onPress={findSimilar} />}
        {similar.state === "running" && (
          <View style={styles.status}>
            <ActivityIndicator color={c.accent} />
            <Text style={[type.body, { color: c.ink }]}>Finding similar photos…</Text>
          </View>
        )}
        {similar.state === "failed" && (
          <>
            <Text style={[type.footnote, { color: c.ink2 }]}>
              Finding similar photos failed: {similar.message}
            </Text>
            <Button label="Try again" onPress={findSimilar} />
          </>
        )}
        {similar.state === "done" && (
          <>
            <View style={styles.summary}>
              <Text style={[type.footnote, styles.fill, { color: c.ink2 }]}>
                {summary(similar.analysis)}
              </Text>
              <TextButton label="Find again" onPress={findSimilar} />
            </View>
            {similar.analysis.groups.length > 0 && (
              <Segmented options={SHOW} value={show} onChange={setShow} />
            )}
          </>
        )}
      </View>
    </View>
  );
}

/** "12 groups of similar photos · 5,000 checked". */
function summary(analysis: Analysis): string {
  const n = analysis.groups.length;
  const found =
    n === 0
      ? "No similar photos"
      : `${number.format(n)} ${n === 1 ? "group" : "groups"} of similar photos`;
  const stopped = analysis.cancelled ? " · stopped early" : "";
  return `${found} · ${number.format(analysis.analyzed)} checked${stopped}`;
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  header: {
    flexDirection: "row",
    alignItems: "flex-end",
    paddingHorizontal: space.edge,
    paddingTop: space.gap,
    paddingBottom: space.gap,
  },
  group: { paddingHorizontal: space.edge, paddingTop: 16, paddingBottom: 6 },
  bar: { paddingHorizontal: space.edge, paddingVertical: space.gap * 1.5, gap: space.gap },
  status: { minHeight: 50, flexDirection: "row", alignItems: "center", gap: space.gap },
  summary: { flexDirection: "row", alignItems: "center" },
});
