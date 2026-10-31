import { useId, useLayoutEffect, useRef, useState } from "react";
import { lensMap } from "./glass-map";

/** One moving material behind the existing links; never duplicates their content. */
export function NavigationGlass() {
  const id = `lens-${useId().replaceAll(":", "")}`;
  const lens = useRef<HTMLSpanElement>(null);
  const [map, setMap] = useState<ReturnType<typeof lensMap>>(null);
  useLayoutEffect(() => {
    const surface = lens.current;
    const nav = surface?.parentElement;
    if (!surface || !nav) return;
    const preferences = matchMedia(
      "(prefers-reduced-transparency: reduce), (prefers-contrast: more)",
    );
    const supported =
      /(?:Chrome|Chromium|Edg)\//.test(navigator.userAgent) &&
      CSS.supports("backdrop-filter", `url("#${id}")`);
    let frame = 0;
    const sync = () => {
      frame = 0;
      const selected = nav.querySelector('[aria-current="page"]');
      if (!selected) {
        surface.hidden = true;
        return;
      }
      surface.hidden = false;
      const bounds = selected.getBoundingClientRect();
      const parent = nav.getBoundingClientRect();
      const width = bounds.width - 4;
      const height = bounds.height - 26;
      surface.style.left = `${bounds.left - parent.left + 2}px`;
      surface.style.top = `${bounds.top - parent.top + 13}px`;
      surface.style.width = `${width}px`;
      surface.style.height = `${height}px`;
      const enabled =
        supported &&
        !preferences.matches &&
        document.documentElement.getAttribute("data-lg-force-fallback") !== "true";
      const next = enabled ? lensMap(width, height) : null;
      setMap(next);
      surface.style.setProperty("--lens-filter", `url("#${id}")`);
      surface.setAttribute("data-lg-optics", next ? "refractive" : "fallback");
      nav.setAttribute("data-lg-ready", "true");
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(sync);
    };
    const resize = new ResizeObserver(schedule);
    resize.observe(nav);
    const changes = new MutationObserver(schedule);
    changes.observe(nav, { subtree: true, attributes: true, attributeFilter: ["aria-current"] });
    const theme = new MutationObserver(schedule);
    theme.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-lg-force-fallback"],
    });
    const light = (event: PointerEvent) => {
      const rect = surface.getBoundingClientRect();
      surface.style.setProperty(
        "--lens-light-x",
        `${Math.max(0, Math.min(100, ((event.clientX - rect.left) / rect.width) * 100))}%`,
      );
    };
    nav.addEventListener("pointermove", light, { passive: true });
    preferences.addEventListener("change", schedule);
    sync();
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
      changes.disconnect();
      theme.disconnect();
      nav.removeEventListener("pointermove", light);
      preferences.removeEventListener("change", schedule);
      nav.removeAttribute("data-lg-ready");
    };
  }, [id]);
  return (
    <>
      <svg className="glass-defs" aria-hidden="true" focusable="false">
        <defs>
          <filter
            id={id}
            x="-35%"
            y="-35%"
            width="170%"
            height="170%"
            colorInterpolationFilters="sRGB"
          >
            <feImage
              href={map?.url}
              result="lens-map"
              x="0"
              y="0"
              width={map?.width ?? "100%"}
              height={map?.height ?? "100%"}
              preserveAspectRatio="none"
            />
            <feDisplacementMap
              in="SourceGraphic"
              in2="lens-map"
              scale={(map?.scale ?? 0) * 1.1}
              xChannelSelector="R"
              yChannelSelector="G"
              result="displaced"
            />
            <feColorMatrix
              in="displaced"
              type="matrix"
              values="1.05 0 0 0 0  0 1.02 0 0 0  0 0 1.06 0 0  0 0 0 1 0"
            />
          </filter>
        </defs>
      </svg>
      <span
        ref={lens}
        aria-hidden="true"
        className={`nav-lens${map ? " nav-lens-refractive" : ""}`}
      />
    </>
  );
}
