import type { AiKeyView, AiSettingsView } from "@lumen/api-client";
import { isApiKey } from "@/shared/lib";

/** Where AI review sends metadata (ADR-0036). */
export type AiDestination = "jeb" | "openrouter";

/** The judge's name for people, or `null` when no key is stored. */
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
 * Where metadata will go once AI review is on: the stored key's service, or
 * Jeb for a key the user is about to give (the app stores Jeb keys).
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

/** Whether the consent can be given: a key is stored or one is typed in. */
export function canTurnOn(settings: AiSettingsView, keyText: string): boolean {
  return !needsKey(settings) || isApiKey(keyText);
}

/** The service AI review uses, when it is on (and a key is stored); else `null`. */
export function isOn(settings: AiSettingsView): string | null {
  return settings.enabled ? serviceName(settings.key) : null;
}
