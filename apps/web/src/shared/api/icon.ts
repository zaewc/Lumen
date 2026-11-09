import { z } from "zod";

/**
 * The longest icon the page accepts: the host's largest PNG (32 KiB) in
 * base64, with the `data:` prefix.
 */
export const MAX_ICON_URL = 44_000;

/**
 * An app's icon from the host (roadmap 16.4): a small PNG as a `data:` URL,
 * or `null` when there is none. Anything else (another image type, a URL to
 * fetch, an oversized payload) is refused, so the page only ever shows an
 * inline PNG.
 */
export const zAppIcon = z
  .string()
  .max(MAX_ICON_URL)
  .regex(/^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/u)
  .nullable();
