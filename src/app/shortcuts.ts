// Raccourcis globaux, déclarés une seule fois. Les composants les référencent par id.

import type { Locale } from "../core/format";

export type ShortcutId = "palette" | "settings" | "theme" | "back" | "forward";

interface Shortcut {
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt?: boolean;
}

const SHORTCUTS: Record<ShortcutId, Shortcut> = {
  palette: { key: "k", ctrl: true, shift: false },
  settings: { key: ",", ctrl: true, shift: false },
  theme: { key: "l", ctrl: true, shift: true },
  // Explorer, comme un navigateur (Retour arrière et boutons 4/5 de la souris : voir `navigationKey`).
  back: { key: "arrowleft", ctrl: false, shift: false, alt: true },
  forward: { key: "arrowright", ctrl: false, shift: false, alt: true },
};

const KEY_LABELS: Record<string, string> = { arrowleft: "←", arrowright: "→" };

/** Libellé façon maquette : « Ctrl K », « Alt ← ». */
export function shortcutLabel(id: ShortcutId, locale: Locale = "fr"): string {
  const s = SHORTCUTS[id];
  return [s.ctrl ? "Ctrl" : null, s.shift ? (locale === "fr" ? "Maj" : "Shift") : null, s.alt ? "Alt" : null, KEY_LABELS[s.key] ?? s.key.toUpperCase()]
    .filter(Boolean)
    .join(" ");
}

export function matchShortcut(e: KeyboardEvent, id: ShortcutId): boolean {
  const s = SHORTCUTS[id];
  return e.key.toLowerCase() === s.key && (e.ctrlKey || e.metaKey) === s.ctrl && e.shiftKey === s.shift && e.altKey === Boolean(s.alt);
}

/** Saisie de texte en cours : Retour arrière y efface, il ne navigue pas. */
function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/** Précédent / suivant de l'Explorer : Alt+← / Alt+→, ou Retour arrière (hors saisie). */
export function navigationKey(e: KeyboardEvent): "back" | "forward" | null {
  if (matchShortcut(e, "back")) return "back";
  if (matchShortcut(e, "forward")) return "forward";
  if (e.key === "Backspace" && !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && !typing(e.target)) return "back";
  return null;
}

/** Boutons 4 (précédent) et 5 (suivant) de la souris. */
export function navigationButton(e: MouseEvent): "back" | "forward" | null {
  if (e.button === 3) return "back";
  if (e.button === 4) return "forward";
  return null;
}
