import { createContext, use } from "react";

import type { Api } from "./api";

export type { Api } from "./api";
export { DEFAULT_MODEL_SETTINGS, errorKind, errorMessage } from "./api";
export * from "./types";

/** The backend the UI talks to; tests and the mock dev mode swap it. */
export const ApiContext = createContext<Api | null>(null);

export function useApi(): Api {
  const api = use(ApiContext);
  if (!api) throw new Error("useApi outside <ApiContext>");
  return api;
}

/**
 * The backend for this build: the in-memory mock when the app runs outside
 * Tauri with `VITE_MOCK=1` (browser development, e2e), else Tauri.
 */
export async function loadApi(): Promise<Api> {
  if (import.meta.env.VITE_MOCK === "1") {
    const { createMockApi } = await import("./mock");
    return createMockApi({ delayMs: 120 });
  }
  const { tauriApi } = await import("./tauri");
  return tauriApi;
}
