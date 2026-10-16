import { createContext, useContext } from "react";
import type { LumenTransport } from "./transport";

/** The transport the app was started with (Tauri, HTTP or in-memory). */
export const TransportContext = createContext<LumenTransport | null>(null);

/** The app's transport; only valid inside the app's providers. */
export function useTransport(): LumenTransport {
  const transport = useContext(TransportContext);
  if (transport === null) {
    throw new Error("useTransport outside the app's providers");
  }
  return transport;
}
