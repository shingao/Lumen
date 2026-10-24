// Anonymisation des chemins d'un rapport (cochée par défaut : un rapport est fait pour être
// partagé). `C:\Users\<nom>` devient `%USERPROFILE%` (le profil des autres comptes : `C:\Users\•••`),
// et tout ce qui est sous un dossier personnel (Documents, Bureau, Images…) est masqué : chaque
// nom devient `•••`, l'extension d'un fichier est gardée (`•••.pdf`). Logique pure.

export const MASK = "•••";

/** Dossiers personnels, à la racine d'un profil (noms réels sur le disque, en majuscules). */
const PERSONAL = new Set([
  "DESKTOP",
  "DOCUMENTS",
  "DOWNLOADS",
  "PICTURES",
  "VIDEOS",
  "MUSIC",
  "FAVORITES",
  "CONTACTS",
  "LINKS",
  "SEARCHES",
  "ICLOUDDRIVE",
  "ICLOUD PHOTOS",
  "DROPBOX",
  "GOOGLE DRIVE",
  "MY DRIVE",
]);

function isPersonal(name: string): boolean {
  const n = name.toUpperCase();
  return PERSONAL.has(n) || n === "ONEDRIVE" || n.startsWith("ONEDRIVE -") || n.startsWith("ONEDRIVE-");
}

export interface AnonContext {
  /** Profil de l'utilisateur (`C:\Users\Arthur`), s'il est connu. */
  profile: string | null;
  /** Fichier (sinon dossier) : son extension est gardée. */
  file?: boolean;
}

function maskName(name: string, file: boolean): string {
  if (!file) return MASK;
  const dot = name.lastIndexOf(".");
  // Extension courte seulement (`.pdf`, `.docx`) : jamais un morceau de nom.
  return dot > 0 && name.length - dot <= 6 && /^\.[A-Za-z0-9]+$/.test(name.slice(dot)) ? `${MASK}${name.slice(dot).toLowerCase()}` : MASK;
}

/** Masque ce qui suit un dossier personnel (`rest[0]`). */
function maskBelowPersonal(rest: string[], file: boolean): string[] {
  if (rest.length === 0 || !isPersonal(rest[0] ?? "")) return rest;
  return rest.map((c, i) => (i === 0 ? c : maskName(c, file && i === rest.length - 1)));
}

function startsWithFolded(path: string, prefix: string): boolean {
  return path.length >= prefix.length && path.slice(0, prefix.length).toUpperCase() === prefix.toUpperCase() && (path.length === prefix.length || path[prefix.length] === "\\");
}

/** Chemin anonymisé. Accepte les chemins complets, `~\…` (forme affichée) et `%USERPROFILE%\…`. */
export function anonymizePath(path: string, ctx: AnonContext): string {
  const file = ctx.file ?? false;
  const profile = ctx.profile?.replace(/\\+$/, "") ?? null;
  let rest: string[] | null = null;
  if (path === "~" || path.startsWith("~\\")) rest = path.slice(2).split("\\").filter(Boolean);
  else if (startsWithFolded(path, "%USERPROFILE%")) rest = path.slice("%USERPROFILE%".length).split("\\").filter(Boolean);
  else if (profile && startsWithFolded(path, profile)) rest = path.slice(profile.length).split("\\").filter(Boolean);
  if (rest) return ["%USERPROFILE%", ...maskBelowPersonal(rest, file)].join("\\");
  // Profil d'un autre compte : `X:\Users\<nom>\…`.
  const m = /^([A-Za-z]:\\Users)\\([^\\]+)(\\.*)?$/i.exec(path);
  if (m) {
    const base = m[1] ?? "";
    const tail = (m[3] ?? "").split("\\").filter(Boolean);
    const shared = ["PUBLIC", "DEFAULT", "ALL USERS", "DEFAULT USER"].includes((m[2] ?? "").toUpperCase());
    return [base, shared ? (m[2] ?? "") : MASK, ...maskBelowPersonal(tail, file)].join("\\");
  }
  return path;
}

/** Nom d'un élément d'après son chemin anonymisé (dernier composant). */
export function anonymizeName(path: string, name: string, ctx: AnonContext): string {
  const anon = anonymizePath(path, ctx);
  if (anon === path) return name;
  const last = anon.split("\\").pop() ?? name;
  return last === "%USERPROFILE%" ? last : last.startsWith(MASK) ? last : name;
}
