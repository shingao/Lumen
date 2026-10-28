// Timeline factice : mêmes formes que les commandes Rust (src-tauri/src/timeline/). Historique
// synthétique de 4 mois déjà passé par la rétention (tout sur 7 jours, un par jour sur 30 jours,
// un par semaine au-delà), un nettoyage de Lumen marqué sur la courbe, une clé USB qui a changé
// de lettre, un disque absent et une série en MFT non comparable à celle du parcours.

import { GIB } from "../../core/format";
import type {
  DailyStatus,
  DatabaseInfo,
  FolderChange,
  ScanTarget,
  SeriesKey,
  TimelineCompare,
  TimelineData,
  TimelineMarker,
  TimelinePoint,
  TimelineSeries,
} from "../types";

const DAY = 86_400_000;
const gb = (n: number) => Math.round(n * GIB);
const PROFILE = "\\Users\\Arthur";

interface MockSeries {
  info: Omit<TimelineSeries, "count" | "first" | "last" | "lastId">;
  /** Âges des snapshots (jours) et espace utilisé en Gio. */
  points: [age: number, used: number, source: TimelinePoint["source"]][];
  /** Dossiers (chemins relatifs) : taille en Gio selon l'âge. */
  folders: Record<string, (age: number) => number | null>;
  free: number | null;
  total: number | null;
  base: string | null;
  target: ScanTarget | null;
  markers: { age: number; bytes: number; title: string; kind: TimelineMarker["kind"] }[];
  letters?: (age: number) => string;
}

/** Âges retenus après rétention, du plus ancien au plus récent. */
function retained(days: number, perDayRecent: number): number[] {
  const ages: number[] = [];
  for (let a = days; a >= 30; a -= 7) ages.push(a);
  for (let a = 29; a >= 7; a--) ages.push(a);
  for (let a = 6; a >= 0; a--) for (let k = perDayRecent - 1; k >= 0; k--) ages.push(a + k * 0.3);
  return ages;
}

/** Profil : +1,1 Gio par jour, un jeu installé il y a 5 jours (+9 Gio), nettoyage Lumen il y a 12 jours (−44,2 Gio). */
function profileUsed(age: number): number {
  const base = 230 - age * 1.1 + Math.sin(age * 1.7) * 0.6;
  return base - (age < 12 ? 44.2 : 0) + (age < 5 ? 9 : 0);
}

const SERIES: MockSeries[] = [
  {
    info: { volumeId: 1, root: PROFILE, method: "walk", label: "Système", fileSystem: "NTFS", letter: "C:", lastLetter: "C:", profile: true, target: { kind: "userProfile" } },
    points: retained(120, 2).map((a) => [a, profileUsed(a), a % 1 === 0 && a > 0 ? "daily" : "scan"]),
    folders: {
      "AppData": (a) => 61 - a * 0.35 - (a < 12 ? 30.1 : 0),
      "AppData\\Local": (a) => 55 - a * 0.33 - (a < 12 ? 30.1 : 0),
      "AppData\\Local\\npm-cache": (a) => 4 + Math.max(0, 40 - a) * 0.24 + (a >= 12 ? 9.4 : 0),
      "AppData\\Local\\Docker": (a) => 14 + Math.max(0, 60 - a) * 0.05,
      "AppData\\Local\\Temp": (a) => (a < 12 ? 1.2 : 8.4 - a * 0.02),
      "Vidéos": (a) => 120 - a * 0.4,
      "Vidéos\\Captures": (a) => 82 - a * 0.4,
      "Vidéos\\Captures\\2025": (a) => 52 - a * 0.38,
      "Projets": (a) => 46 - a * 0.1,
      "Projets\\kaiseki-api": (a) => 6 + Math.max(0, 35 - a) * 0.12,
      "Projets\\_archives": (a) => (a < 12 ? 2.1 : 6.6),
      "SteamLibrary": (a) => (a < 5 ? 9.1 : null),
      "Téléchargements": (a) => 9 + (a > 20 ? 3.4 : 0),
      "Téléchargements\\ISO": (a) => (a > 20 ? 3.4 : null),
    },
    free: 73.8,
    total: 476,
    base: "C:\\Users\\Arthur",
    target: { kind: "userProfile" },
    markers: [
      { age: 12, bytes: 44.2, title: "Cache npm, Caches Gradle, Fichiers temporaires", kind: "recos" },
      { age: 47, bytes: 6.1, title: "Doublons", kind: "duplicates" },
    ],
  },
  {
    info: { volumeId: 1, root: "\\", method: "walk", label: "Système", fileSystem: "NTFS", letter: "C:", lastLetter: "C:", profile: false, target: { kind: "volume", letter: "C:" } },
    points: [20, 13, 9, 4, 1, 0].map((a) => [a, 402 - a * 0.2 + (a < 4 ? 1.4 : 0), "scan"]),
    folders: { "Users": (a) => 300 - a * 0.2, "Windows": () => 31, "Program Files": (a) => 22 + (a < 4 ? 1.4 : 0) },
    free: 73.8,
    total: 476,
    base: "C:\\",
    target: { kind: "volume", letter: "C:" },
    markers: [],
  },
  {
    info: { volumeId: 1, root: PROFILE, method: "mft", label: "Système", fileSystem: "NTFS", letter: "C:", lastLetter: "C:", profile: true, target: { kind: "userProfile" } },
    points: [[3, profileUsed(3), "scan"], [1, profileUsed(1), "scan"]],
    folders: { "AppData": (a) => 61 - a * 0.35 - 30.1 },
    free: 73.8,
    total: 476,
    base: "C:\\Users\\Arthur",
    target: { kind: "userProfile" },
    markers: [],
  },
  {
    // Clé USB analysée en E:, aujourd'hui en F: : même GUID, même série.
    info: { volumeId: 2, root: "\\", method: "walk", label: "Photos", fileSystem: "exFAT", letter: "F:", lastLetter: "E:", profile: false, target: { kind: "volume", letter: "F:" } },
    points: [60, 32, 18, 2].map((a) => [a, 41 + (60 - a) * 0.15, "scan"]),
    folders: { "DCIM": (a) => 38 + (60 - a) * 0.15 },
    free: 72,
    total: 119,
    base: "F:\\",
    target: { kind: "volume", letter: "F:" },
    markers: [],
    letters: () => "E:",
  },
  {
    info: { volumeId: 3, root: "\\", method: "walk", label: "Sauvegardes", fileSystem: "NTFS", letter: null, lastLetter: "G:", profile: false, target: null },
    points: [90, 61, 30].map((a) => [a, 610 + (90 - a) * 0.4, "scan"]),
    folders: { "Backups": (a) => 600 + (90 - a) * 0.4 },
    free: 321,
    total: 931,
    base: null,
    target: null,
    markers: [],
  },
];

const sameKey = (a: SeriesKey, b: SeriesKey) => a.volumeId === b.volumeId && a.root.toLowerCase() === b.root.toLowerCase() && a.method === b.method;

function slope(xy: [number, number][]): number | null {
  const n = xy.length;
  const mx = xy.reduce((s, [x]) => s + x / n, 0);
  const my = xy.reduce((s, [, y]) => s + y / n, 0);
  let num = 0;
  let den = 0;
  for (const [x, y] of xy) {
    num += (x - mx) * (y - my);
    den += (x - mx) ** 2;
  }
  return den > 0 ? num / den : null;
}

export function createMockTimeline(now: () => number) {
  /** Identifiants de snapshots : série × 1000 + rang. */
  const pointsOf = (s: MockSeries, si: number): (TimelinePoint & { age: number })[] =>
    s.points.map(([age, used, source], i) => ({ id: si * 1000 + i, at: Math.round(now() - age * DAY), used: gb(used), source, age }));

  const find = (key: SeriesKey) => SERIES.findIndex((s) => sameKey(s.info, key));

  function data(key: SeriesKey): TimelineData {
    const si = find(key);
    const s = SERIES[si];
    if (!s) return { points: [], markers: [], projection: { status: "tooFew", points: 0, bytesPerDay: null, daysToFull: null }, free: null, total: null, trend7d: null, base: null, target: null };
    const pts = pointsOf(s, si);
    const markers: TimelineMarker[] = s.markers.map((m, i) => ({ actionId: 100 + i, at: Math.round(now() - m.age * DAY), bytes: gb(m.bytes), title: m.title, kind: m.kind }));
    // Même calcul que le backend : 30 jours, au moins 5 points, nettoyages rajoutés.
    const win = pts.filter((p) => p.at >= now() - 30 * DAY);
    const first = win[0]?.at ?? 0;
    const xy = win.map((p): [number, number] => [(p.at - first) / DAY, p.used + markers.filter((m) => m.at > first && m.at <= p.at).reduce((a, m) => a + m.bytes, 0)]);
    const per = win.length >= 5 ? slope(xy) : null;
    const free = s.free === null ? null : gb(s.free);
    const projection: TimelineData["projection"] =
      win.length < 5 || per === null
        ? { status: "tooFew", points: win.length, bytesPerDay: null, daysToFull: null }
        : per < 1024 * 1024
          ? { status: "flat", points: 0, bytesPerDay: per, daysToFull: null }
          : { status: "growing", points: win.length, bytesPerDay: per, daysToFull: free === null ? null : free / per };
    const last = pts[pts.length - 1];
    const before = [...pts].reverse().find((p) => p.at <= now() - 7 * DAY);
    return {
      points: pts.map(({ id, at, used, source }) => ({ id, at, used, source })),
      markers: markers.filter((m) => m.at >= (pts[0]?.at ?? 0)),
      projection,
      free,
      total: s.total === null ? null : gb(s.total),
      trend7d: last && before ? last.used - before.used : null,
      base: s.base,
      target: s.target,
    };
  }

  function compare(from: number, to: number): TimelineCompare {
    const locate = (id: number) => {
      const si = Math.floor(id / 1000);
      const s = SERIES[si] as MockSeries;
      const p = pointsOf(s, si)[id % 1000];
      if (!p) throw new Error("snapshot introuvable");
      return { s, p };
    };
    let a = locate(from);
    let b = locate(to);
    if (a.p.at > b.p.at) [a, b] = [b, a];
    const ref = (x: { s: MockSeries; p: TimelinePoint & { age: number } }) => ({
      id: x.p.id,
      at: x.p.at,
      used: x.p.used,
      root: x.s.info.root,
      method: x.s.info.method,
      letter: x.s.letters?.(x.p.age) ?? x.s.info.letter ?? x.s.info.lastLetter,
    });
    const reason = a.s.info.volumeId !== b.s.info.volumeId ? "volume" : a.s.info.root.toLowerCase() !== b.s.info.root.toLowerCase() ? "root" : a.s.info.method !== b.s.info.method ? "method" : null;
    const base = { from: ref(a), to: ref(b), minBytes: 1024 * 1024 };
    if (reason) return { ...base, comparable: false, reason, diff: null, base: null, target: null };
    const changes: FolderChange[] = Object.entries(a.s.folders).map(([path, f]) => {
      const x = f(a.p.age);
      const y = f(b.p.age);
      return { path, before: x === null ? null : gb(x), after: y === null ? null : gb(y), delta: gb(y ?? 0) - gb(x ?? 0) };
    });
    // Un parent n'est pas cité si un seul sous-dossier porte l'essentiel de sa variation.
    const explained = (c: FolderChange) =>
      changes.some((o) => o.path.startsWith(`${c.path}\\`) && !o.path.slice(c.path.length + 1).includes("\\") && o.before !== null && o.after !== null && Math.sign(o.delta) === Math.sign(c.delta) && Math.abs(o.delta) >= 0.9 * Math.abs(c.delta));
    const both = changes.filter((c) => c.before !== null && c.after !== null && Math.abs(c.delta) >= 64 * 1024 * 1024 && !explained(c));
    const top = (v: FolderChange[], k: (c: FolderChange) => number) => v.sort((x, y) => k(y) - k(x)).slice(0, 15);
    return {
      ...base,
      comparable: true,
      reason: null,
      diff: {
        grown: top(both.filter((c) => c.delta > 0), (c) => c.delta),
        shrunk: top(both.filter((c) => c.delta < 0), (c) => -c.delta),
        added: top(changes.filter((c) => c.before === null && c.after !== null), (c) => c.delta),
        removed: top(changes.filter((c) => c.before !== null && c.after === null), (c) => -c.delta),
        total: b.p.used - a.p.used,
      },
      base: a.s.base,
      target: a.s.target,
    };
  }

  let purged = false;
  const listeners = new Set<() => void>();

  return {
    series(): TimelineSeries[] {
      if (purged) return [];
      return SERIES.map((s, si) => {
        const pts = pointsOf(s, si);
        return { ...s.info, count: pts.length, first: pts[0]?.at ?? 0, last: pts[pts.length - 1]?.at ?? 0, lastId: pts[pts.length - 1]?.id ?? 0 };
      }).sort((x, y) => y.last - x.last);
    },
    data: (key: SeriesKey) => (purged ? data({ volumeId: -1, root: "", method: "walk" }) : data(key)),
    compare,
    trend(target: ScanTarget): number | null {
      const key: SeriesKey = target.kind === "userProfile" ? { volumeId: 1, root: PROFILE, method: "walk" } : { volumeId: target.letter.toUpperCase() === "C:" ? 1 : -1, root: "\\", method: "walk" };
      return purged ? null : data(key).trend7d;
    },
    daily(enabled: boolean): DailyStatus {
      return enabled
        ? { state: "idle", label: "C:\\Users\\Arthur", lastAt: now() - 3 * 3_600_000, lastOutcome: "done", needsAdmin: 1 }
        : { state: "off", label: null, lastAt: null, lastOutcome: null, needsAdmin: 0 };
    },
    database(): DatabaseInfo {
      const snapshots = purged ? 0 : SERIES.reduce((a, s) => a + s.points.length, 0);
      return { bytes: purged ? 98_304 : 2_621_440 + snapshots * 18_432 + 41_943_040, snapshots, series: purged ? 0 : SERIES.length, savedScans: purged ? 0 : 2, savedBytes: purged ? 0 : 41_943_040 };
    },
    purge(): DatabaseInfo {
      purged = true;
      listeners.forEach((l) => l());
      return this.database();
    },
    onChanged(l: () => void) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
  };
}
