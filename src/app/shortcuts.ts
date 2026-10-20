// Raccourcis globaux, déclarés une seule fois. Les composants les référencent par id.

import type { Locale } from "../core/format";

export type ShortcutId = "palette" | "settings" | "theme";

interface Shortcut {
  key: string;
  ctrl: boolean;
  shift: boolean;
}

const SHORTCUTS: Record<ShortcutId, Shortcut> = {
  palette: { key: "k", ctrl: true, shift: false },
  settings: { key: ",", ctrl: true, shift: false },
  theme: { key: "l", ctrl: true, shift: true },
};

/** Libellé façon maquette : « Ctrl K ». */
export function shortcutLabel(id: ShortcutId, locale: Locale = "fr"): string {
  const s = SHORTCUTS[id];
  return [s.ctrl ? "Ctrl" : null, s.shift ? (locale === "fr" ? "Maj" : "Shift") : null, s.key.toUpperCase()].filter(Boolean).join(" ");
}

export function matchShortcut(e: KeyboardEvent, id: ShortcutId): boolean {
  const s = SHORTCUTS[id];
  return e.key.toLowerCase() === s.key && (e.ctrlKey || e.metaKey) === s.ctrl && e.shiftKey === s.shift && !e.altKey;
}
