import { useSyncExternalStore } from "react";
let revision = 0;
const listeners = new Set<() => void>();
export const changed = () => {
  revision++;
  for (const listener of listeners) listener();
};
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export const useEditorState = () =>
  useSyncExternalStore(subscribe, () => revision);
export const view = {
  tab: "route",
  status: "Loading roads…",
  mode: "draw",
  hint: "",
  network: "",
  progress: "",
  details: false,
  role: "spline_point",
  filter: "",
  samples: false,
  snap: true,
  busy: false,
  ready: false,
  message: "",
  error: false,
  download: "",
};
export function notice(text: string, bad = false, download = "") {
  Object.assign(view, { message: text, error: bad, download });
  changed();
}
export function safely<T extends unknown[]>(fn: (...args: T) => unknown) {
  return async (...args: T) => {
    try {
      await fn(...args);
    } catch (e) {
      notice(e instanceof Error ? e.message : String(e), true);
    } finally {
      changed();
    }
  };
}
export const errorMessage = (e: unknown) =>
  e instanceof Error ? e.message : String(e);
export const actions: Record<string, () => Promise<void>> = {};
export function action(name: string, fn: () => unknown) {
  actions[name] = safely(fn);
}
