import { Fragment, type ReactNode, useId, useLayoutEffect, useRef } from "react";
import {
  bendScale,
  engineOf,
  type Lens,
  type LensOptics,
  lensMap,
  squeezeMatrix,
} from "./glass-map";

/**
 * Indicator optics. The curved band reaches the icon and the label's ends; the
 * linear encoding (spread 1) keeps the bend's fall smooth.
 */
const OPTICS: LensOptics = { bezel: 0.8, spread: 1, specular: 0.6, rim: 3, light: -120 };
/**
 * Largest local magnification at the rim. A bend at the field's measured fold limit
 * would stretch one column across many (a smear) and beyond it repeat content;
 * at most 2x the rim visibly magnifies while labels stay legible.
 */
const MAX_MAGNIFY = 2;
/** Largest rim bend in CSS px, like the skill's segmented indicator. */
const MAX_BEND = 5;
/** Colour fringe at the rim: blue bends this much further than red. */
const CHROMA = 0.08;
/** Re-issue delays after a map swap: WebKit loads its own copy and repaints late. */
const REISSUE_MS = [60, 250] as const;
/** The lens slide, matching --material-response. */
const SLIDE_MS = 160;
const IDENTITY = "1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 1 0";

/** One pass per colour channel; each keeps its channel and the full alpha. */
const CHANNELS = [
  ["r", "1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0", 1],
  ["g", "0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0", 1 + CHROMA / 2],
  ["b", "0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0", 1 + CHROMA],
] as const;

interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

function userAgentBrands(): readonly string[] {
  const data = (navigator as Navigator & { userAgentData?: { brands?: { brand: string }[] } })
    .userAgentData;
  return data?.brands?.map((entry) => entry.brand) ?? [];
}

/**
 * The selected place's lens. `children` is an aria-hidden copy of the navigation
 * list (the same items, all drawn as selected) laid exactly over the real one.
 * The copy is bent with `filter: url()`, which Chromium, WebKit and Gecko all
 * render, and clipped to the lens; `backdrop-filter: url()` is Chromium-only.
 * The real links underneath stay the interactive, accessible elements.
 */
export function NavigationGlass({ children }: { readonly children: ReactNode }) {
  const base = `lens-${useId().replace(/[^\w-]/g, "")}`;
  const filterRef = useRef<SVGFilterElement>(null);
  const shadowRef = useRef<HTMLSpanElement>(null);
  const copyRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const filter = filterRef.current;
    const shadow = shadowRef.current;
    const copy = copyRef.current;
    const nav = shadow?.parentElement;
    const list = nav?.querySelector<HTMLElement>(":scope > ul");
    const image = filter?.querySelector("feImage");
    const squeeze = filter?.querySelector<SVGFEColorMatrixElement>('[result="bend"]');
    if (!filter || !shadow || !copy || !nav || !list || !image || !squeeze) return;
    const shifts = [...filter.querySelectorAll("feDisplacementMap")];
    const primitives = [...filter.children];
    const preferences = matchMedia(
      "(prefers-reduced-transparency: reduce), (prefers-contrast: more), (forced-colors: active)",
    );
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    const engine = engineOf(navigator.userAgent, userAgentBrands());
    let frame = 0;
    let slide = 0;
    let version = 0;
    let request = 0;
    let shape = "";
    let map: Lens | null = null;
    let box = { left: 0, top: 0, width: 0, height: 0 };
    let shown: Rect | null = null;
    let target: Rect | null = null;
    let selected: Element | null = null;
    let lightX = 0.26;
    const timers = new Set<number>();
    const clearTimers = () => {
      for (const timer of timers) clearTimeout(timer);
      timers.clear();
    };

    /** A new filter id: WebKit caches filter output per id and repaints only for a new one. */
    const issue = () => {
      if (!map) return;
      version += 1;
      filter.id = `${base}-v${version}`;
      copy.style.filter = `url("#${filter.id}")`;
    };
    const reissueLater = () => {
      clearTimers();
      for (const delay of REISSUE_MS) {
        const timer = window.setTimeout(() => {
          timers.delete(timer);
          issue();
        }, delay);
        timers.add(timer);
      }
    };

    /** Places the shadow, the clip and the map's sub-region; never redraws the map. */
    const paint = (lens: Rect) => {
      shown = lens;
      shadow.style.left = `${box.left + lens.x}px`;
      shadow.style.top = `${box.top + lens.y}px`;
      shadow.style.width = `${lens.width}px`;
      shadow.style.height = `${lens.height}px`;
      if (!map || box.width <= 0 || box.height <= 0) return;
      const right = box.width - lens.x - lens.width;
      const bottom = box.height - lens.y - lens.height;
      copy.style.clipPath = `inset(${lens.y}px ${right}px ${bottom}px ${lens.x}px round ${lens.height / 2}px)`;
      copy.style.setProperty("--lens-light-x", `${lens.x + lightX * lens.width}px`);
      copy.style.setProperty("--lens-light-y", `${lens.y}px`);
      copy.style.setProperty("--lens-w", `${lens.width}px`);
      copy.style.setProperty("--lens-h", `${lens.height}px`);
      // Every primitive works on the lens (plus a pixel for the clip's anti-aliasing).
      const fraction = (rect: Rect) => ({
        x: String(rect.x / box.width),
        y: String(rect.y / box.height),
        width: String(rect.width / box.width),
        height: String(rect.height / box.height),
      });
      const padded = fraction({
        x: lens.x - 1,
        y: lens.y - 1,
        width: lens.width + 2,
        height: lens.height + 2,
      });
      for (const node of primitives) {
        const region = node === image ? fraction(lens) : padded;
        for (const [name, value] of Object.entries(region)) node.setAttribute(name, value);
      }
      issue();
    };

    /** The bend in filter units for the copy's current size. */
    const tune = () => {
      if (!map || !engine) return;
      const bend = bendScale(
        engine,
        Math.min(map.unfolded * (1 - 1 / MAX_MAGNIFY), MAX_BEND),
        box.width,
        box.height,
      );
      squeeze.setAttribute("values", bend.squeeze ? squeezeMatrix(bend.squeeze) : IDENTITY);
      CHANNELS.forEach(([, , factor], index) => {
        shifts[index]?.setAttribute("scale", String(bend.scale * factor));
      });
    };

    const fallback = () => {
      request += 1;
      shape = "";
      map = null;
      clearTimers();
      copy.removeAttribute("data-lg-shown");
      copy.style.removeProperty("filter");
      shadow.setAttribute("data-lg-optics", "fallback");
    };

    const glide = (from: Rect, to: Rect) => {
      cancelAnimationFrame(slide);
      const start = performance.now();
      const step = (now: number) => {
        const t = Math.min(1, (now - start) / SLIDE_MS);
        const eased = 1 - (1 - t) ** 3;
        const mix = (a: number, b: number) => a + (b - a) * eased;
        slide = t < 1 ? requestAnimationFrame(step) : 0;
        paint({
          x: mix(from.x, to.x),
          y: mix(from.y, to.y),
          width: mix(from.width, to.width),
          height: mix(from.height, to.height),
        });
        if (!slide) reissueLater();
      };
      slide = requestAnimationFrame(step);
    };

    const sync = () => {
      frame = 0;
      const current = list.querySelector('[aria-current="page"]');
      if (!current) {
        shadow.hidden = true;
        copy.removeAttribute("data-lg-shown");
        selected = null;
        target = null;
        return;
      }
      shadow.hidden = false;
      const ratio = window.devicePixelRatio || 1;
      const snap = (value: number) => Math.round(value * ratio) / ratio;
      const bounds = current.getBoundingClientRect();
      const row = list.getBoundingClientRect();
      const parent = nav.getBoundingClientRect();
      // The copy covers exactly the real list, so its labels register with the links.
      box = {
        left: row.left - parent.left,
        top: row.top - parent.top,
        width: row.width,
        height: row.height,
      };
      copy.style.left = `${box.left}px`;
      copy.style.top = `${box.top}px`;
      copy.style.width = `${row.width}px`;
      copy.style.height = `${row.height}px`;
      const next: Rect = {
        x: snap(bounds.left + 2) - row.left,
        y: snap(bounds.top + 13) - row.top,
        width: snap(bounds.width - 4),
        height: snap(bounds.height - 26),
      };
      const moved = selected !== null && selected !== current && shown !== null;
      selected = current;
      target = next;
      nav.setAttribute("data-lg-ready", "true");
      const enabled =
        engine !== null &&
        !preferences.matches &&
        document.documentElement.getAttribute("data-lg-force-fallback") !== "true";
      if (!enabled) {
        fallback();
        cancelAnimationFrame(slide);
        slide = 0;
        paint(next);
        return;
      }
      tune();
      if (moved && shown && !motion.matches) glide(shown, next);
      else if (!slide) {
        paint(next);
        reissueLater();
      }
      const key = `${Math.round(next.width * 2)}:${Math.round(next.height * 2)}`;
      if (key === shape) return;
      shape = key;
      const lens = lensMap(next.width, next.height, OPTICS);
      if (!lens) {
        fallback();
        return;
      }
      // Decode before use: WebKit otherwise caches an empty map for the id.
      const ticket = ++request;
      const decoder = new Image();
      decoder.src = lens.url;
      decoder.decode().then(
        () => {
          if (ticket !== request) return;
          image.setAttribute("href", lens.url);
          map = lens;
          tune();
          copy.setAttribute("data-lg-shown", "true");
          shadow.setAttribute("data-lg-optics", "refractive");
          if (!slide && target) paint(target);
          reissueLater();
        },
        () => {
          if (ticket === request) fallback();
        },
      );
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(sync);
    };
    const resize = new ResizeObserver(schedule);
    resize.observe(nav);
    for (const item of list.children) resize.observe(item);
    const changes = new MutationObserver(schedule);
    changes.observe(list, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: ["aria-current"],
    });
    const root = new MutationObserver(schedule);
    root.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme", "data-lg-force-fallback", "lang"],
    });
    let lightFrame = 0;
    const light = (event: PointerEvent) => {
      const rect = shadow.getBoundingClientRect();
      lightX = Math.max(0, Math.min(1, (event.clientX - rect.left) / (rect.width || 1)));
      shadow.style.setProperty("--lens-light-x", `${lightX * 100}%`);
      if (!lightFrame && !slide) {
        lightFrame = requestAnimationFrame(() => {
          lightFrame = 0;
          if (shown && !slide) paint(shown);
        });
      }
    };
    nav.addEventListener("pointermove", light, { passive: true });
    preferences.addEventListener("change", schedule);
    void document.fonts.ready.then(schedule);
    sync();
    return () => {
      request += 1;
      cancelAnimationFrame(frame);
      cancelAnimationFrame(slide);
      cancelAnimationFrame(lightFrame);
      clearTimers();
      resize.disconnect();
      changes.disconnect();
      root.disconnect();
      nav.removeEventListener("pointermove", light);
      preferences.removeEventListener("change", schedule);
      nav.removeAttribute("data-lg-ready");
    };
  }, [base]);
  return (
    <>
      <svg className="glass-defs" aria-hidden="true" focusable="false">
        <defs>
          {/* Dynamic attributes (id, map, regions, scales) are set by the effect above. */}
          <filter
            ref={filterRef}
            x="0"
            y="0"
            width="1"
            height="1"
            filterUnits="objectBoundingBox"
            primitiveUnits="objectBoundingBox"
            colorInterpolationFilters="sRGB"
          >
            <feFlood floodColor="rgb(128,128,0)" result="neutral" />
            <feImage preserveAspectRatio="none" result="shape" />
            <feComposite in="shape" in2="neutral" operator="over" result="map" />
            <feColorMatrix in="map" type="matrix" values={IDENTITY} result="bend" />
            {CHANNELS.map(([channel, matrix]) => (
              <Fragment key={channel}>
                <feDisplacementMap
                  in="SourceGraphic"
                  in2="bend"
                  scale="0"
                  xChannelSelector="R"
                  yChannelSelector="G"
                  result={`shift-${channel}`}
                />
                <feColorMatrix
                  in={`shift-${channel}`}
                  type="matrix"
                  values={matrix}
                  result={channel}
                />
              </Fragment>
            ))}
            <feComposite in="r" in2="g" operator="arithmetic" k2="1" k3="1" result="rg" />
            <feComposite in="rg" in2="b" operator="arithmetic" k2="1" k3="1" result="refracted" />
            <feColorMatrix
              in="map"
              type="matrix"
              values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 1 0 0"
              result="rim"
            />
            <feComposite in="rim" in2="refracted" operator="over" />
          </filter>
        </defs>
      </svg>
      <span ref={shadowRef} aria-hidden="true" className="nav-lens" />
      <div ref={copyRef} aria-hidden="true" inert className="nav-lens-copy">
        {children}
      </div>
    </>
  );
}
