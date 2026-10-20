// Seul point de contact avec Tauri. Les écrans passent par `backend` et `appWindow`.

import type { Backend } from "./backend";
import { createMockBackend } from "./mock";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Jalon 1 : données de la maquette, y compris dans l'appli. Le backend Rust arrive au jalon 3.
export const backend: Backend = createMockBackend();

type TauriWindow = Awaited<ReturnType<typeof loadWindow>>;

async function loadWindow() {
  const mod = await import("@tauri-apps/api/window");
  return mod.getCurrentWindow();
}

let windowPromise: Promise<TauriWindow> | null = null;

function withWindow(action: (w: TauriWindow) => Promise<unknown>): void {
  if (!isTauri) return;
  windowPromise ??= loadWindow();
  windowPromise.then(action).catch((err: unknown) => console.error("fenêtre :", err));
}

export const appWindow = {
  minimize: () => withWindow((w) => w.minimize()),
  toggleMaximize: () => withWindow((w) => w.toggleMaximize()),
  close: () => withWindow((w) => w.close()),
  startDragging: () => withWindow((w) => w.startDragging()),
  onMaximizedChange(listener: (maximized: boolean) => void): () => void {
    if (!isTauri) return () => {};
    let unlisten: (() => void) | null = null;
    let disposed = false;
    withWindow(async (w) => {
      listener(await w.isMaximized());
      const off = await w.onResized(async () => listener(await w.isMaximized()));
      if (disposed) off();
      else unlisten = off;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  },
};
