/**
 * Whether `text`, trimmed, is shaped like an API key the core accepts: 1–256
 * visible ASCII characters (no spaces). Only a first check for the form; the
 * core checks again before storing it.
 */
export function isApiKey(text: string): boolean {
  const key = text.trim();
  return key.length >= 1 && key.length <= 256 && /^[\x21-\x7e]+$/.test(key);
}
