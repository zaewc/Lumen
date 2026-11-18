import type { SoftwareItemView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTransport } from "@/shared/api";

/**
 * A row's app icon (roadmap 16.4), asked for only once the row comes near
 * the screen, and kept for as long as the app's version and location stay
 * the same. A neutral square stands in until it comes, and for software
 * without one. Decorative: the row's name says what it is.
 */
export function AppIcon({ item }: { readonly item: SoftwareItemView }) {
  const ask = useTransport().appIcon;
  const wanted = ask !== undefined && item.origin === "mac_app";
  const slot = useRef<HTMLSpanElement>(null);
  const [near, setNear] = useState(false);
  useEffect(() => {
    const element = slot.current;
    if (!wanted || near || element === null) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) setNear(true);
      },
      { rootMargin: "200px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [wanted, near]);
  const icon = useQuery({
    queryKey: ["app-icon", item.key, item.version, item.location],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask(item.key)),
    enabled: wanted && near,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const src = icon.data ?? null;
  return (
    <span
      ref={slot}
      aria-hidden="true"
      className={`mt-0.5 block size-8 shrink-0 rounded-lg ${src === null ? "bg-fill" : ""}`}
    >
      {src !== null && (
        <img src={src} alt="" width={32} height={32} draggable={false} className="size-8" />
      )}
    </span>
  );
}
