// Filtres de la recherche (miroir de scan::search::parse) : affichés sous le champ de la palette
// et appliqués par le backend factice. `ext:mp4`, `>1go`, `<500mo`, `type:vidéo`,
// `type:dossier`, `disque:D` ; le reste est cherché dans les noms.

import { fold } from "./fuzzy";

export type SearchType = "video" | "image" | "audio" | "archive" | "disk" | "installer" | "document";

export interface SearchQuery {
  words: string[];
  exts: string[];
  minSize: number | null;
  maxSize: number | null;
  types: SearchType[];
  dirsOnly: boolean;
  drives: string[];
}

const UNITS: Record<string, number> = {
  "": 1, o: 1, b: 1, octets: 1, bytes: 1,
  k: 1024, ko: 1024, kb: 1024, kio: 1024, kib: 1024,
  m: 1024 ** 2, mo: 1024 ** 2, mb: 1024 ** 2, mio: 1024 ** 2, mib: 1024 ** 2,
  g: 1024 ** 3, go: 1024 ** 3, gb: 1024 ** 3, gio: 1024 ** 3, gib: 1024 ** 3,
  t: 1024 ** 4, to: 1024 ** 4, tb: 1024 ** 4, tio: 1024 ** 4, tib: 1024 ** 4,
};

/** `1go`, `1,5go`, `500mo` en octets (base 1024), ou null. */
export function parseSize(raw: string): number | null {
  const m = /^(\d+(?:[.,]\d+)?)\s*([a-z]*)$/.exec(raw.trim().toLowerCase());
  if (!m) return null;
  const unit = UNITS[m[2] ?? ""];
  return unit === undefined ? null : Math.round(Number((m[1] ?? "").replace(",", ".")) * unit);
}

const TYPES: Record<string, SearchType | "dir"> = {
  video: "video", videos: "video", film: "video", films: "video",
  image: "image", images: "image", photo: "image", photos: "image",
  audio: "audio", musique: "audio", music: "audio", son: "audio",
  archive: "archive", archives: "archive",
  disque: "disk", disk: "disk", iso: "disk",
  installeur: "installer", installer: "installer", programme: "installer",
  document: "document", documents: "document", doc: "document",
  dossier: "dir", dossiers: "dir", folder: "dir", dir: "dir",
};

export function parseQuery(q: string): SearchQuery {
  const out: SearchQuery = { words: [], exts: [], minSize: null, maxSize: null, types: [], dirsOnly: false, drives: [] };
  for (const token of q.split(/\s+/).filter(Boolean)) {
    const lower = token.toLowerCase();
    const colon = lower.indexOf(":");
    const key = colon > 0 ? lower.slice(0, colon) : "";
    const value = colon > 0 ? lower.slice(colon + 1) : lower;
    let handled = false;
    if (key === "ext" || key === "extension") {
      out.exts.push(...value.split(",").map((e) => fold(e.replace(/^\./, ""))).filter(Boolean));
      handled = true;
    } else if (key === "type") {
      const t = TYPES[fold(value)];
      if (t === "dir") out.dirsOnly = true;
      else if (t) out.types.push(t);
      handled = t !== undefined;
    } else if (["disque", "drive", "lecteur", "disk"].includes(key)) {
      const letter = value.replace(/[:\\]+$/, "");
      if (/^[a-z]$/.test(letter)) {
        out.drives.push(letter.toUpperCase());
        handled = true;
      }
    } else {
      const m = /^(>=|>|<=|<)(.+)$/.exec(lower);
      const size = m ? parseSize(m[2] ?? "") : null;
      if (m && size !== null) {
        if ((m[1] ?? "").startsWith(">")) out.minSize = size;
        else out.maxSize = size;
        handled = true;
      }
    }
    if (!handled) out.words.push(token);
  }
  return out;
}

const EXT_TYPES: Record<SearchType, string[]> = {
  video: ["mp4", "mkv", "mov", "avi", "wmv", "m4v", "webm", "flv", "mpg", "mpeg", "ts", "m2ts", "mts"],
  image: ["jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "heic", "raw", "cr2", "cr3", "nef", "arw", "dng", "psd", "webp"],
  audio: ["mp3", "flac", "wav", "aac", "m4a", "ogg", "wma", "opus", "aiff"],
  archive: ["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "cab"],
  disk: ["iso", "img", "vhd", "vhdx", "vmdk", "vdi", "qcow2", "wim", "esd", "dmg"],
  installer: ["exe", "msi", "msix", "msixbundle", "appx", "appxbundle"],
  document: ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "txt", "csv", "epub"],
};

/** Un élément passe-t-il les filtres (hors mots) ? */
export function matchesFilters(q: SearchQuery, item: { name: string; size: number; kind: "file" | "dir"; drive: string }): boolean {
  if (q.dirsOnly && item.kind !== "dir") return false;
  if (q.minSize !== null && item.size < q.minSize) return false;
  if (q.maxSize !== null && item.size > q.maxSize) return false;
  if (q.drives.length > 0 && !q.drives.some((d) => item.drive.toUpperCase().startsWith(d))) return false;
  if (q.exts.length > 0 || q.types.length > 0) {
    if (item.kind === "dir") return false;
    const ext = fold(item.name.includes(".") ? (item.name.split(".").pop() ?? "") : "");
    if (q.exts.length > 0 && !q.exts.includes(ext)) return false;
    if (q.types.length > 0 && !q.types.some((t) => EXT_TYPES[t].includes(ext))) return false;
  }
  return true;
}
