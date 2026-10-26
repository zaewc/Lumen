import type { AiKeySourceView, AiKeyView, AiSettingsView } from "@lumen/api-client";
import { isApiKey } from "@/shared/lib";

/** Where AI review sends metadata (ADR-0036). */
export type AiDestination = "jeb" | "openrouter";

/** The key AI review uses, for people: its service and whose key it is. */
export type AiKeyInUse = {
  readonly service: string;
  readonly source: AiKeySourceView;
};

/** The judge's name for people, or `null` when there is no key. */
export function serviceName(key: AiKeyView): string | null {
  switch (key) {
    case "jeb":
      return "Jeb";
    case "openrouter":
      return "OpenRouter";
    case "none":
      return null;
  }
}

/** Whether turning AI review on first needs a key from the user. */
export function needsKey(settings: AiSettingsView): boolean {
  return settings.key === "none";
}

/**
 * Whether the user may give their own key even though one is available:
 * only while the key in use is Lumen's built-in one (ADR-0042).
 */
export function offersOwnKey(settings: AiSettingsView): boolean {
  return settings.key_source === "builtin";
}

/**
 * Where metadata will go once AI review is on: the stored key's service, or
 * Jeb for the built-in key or a key the user is about to give (the app
 * stores Jeb keys).
 */
export function destination(settings: AiSettingsView): AiDestination {
  switch (settings.key) {
    case "openrouter":
      return "openrouter";
    case "jeb":
    case "none":
      return "jeb";
  }
}

/**
 * Whose key the request will carry once the consent is given: Lumen's
 * built-in key unless the user typed their own, or there is none built in.
 */
export function sourceOnConsent(settings: AiSettingsView, keyText: string): AiKeySourceView {
  return offersOwnKey(settings) && keyText.trim() === "" ? "builtin" : "stored";
}

/**
 * Whether the consent can be given: a key is available or a valid one is
 * typed in. A key typed next to the built-in one must be valid too, so it
 * is never silently ignored.
 */
export function canTurnOn(settings: AiSettingsView, keyText: string): boolean {
  if (needsKey(settings)) {
    return isApiKey(keyText);
  }
  return keyText.trim() === "" || isApiKey(keyText);
}

/** The key AI review uses, when it is on (and has a key); else `null`. */
export function isOn(settings: AiSettingsView): AiKeyInUse | null {
  const service = serviceName(settings.key);
  if (!settings.enabled || service === null || settings.key_source === null) {
    return null;
  }
  return { service, source: settings.key_source };
}
