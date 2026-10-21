// Nettoyage avancé factice : mêmes formes que les commandes Rust (src-tauri/src/advanced/).

import { GIB } from "../../core/format";
import type {
  CleanRequest,
  DormantResult,
  DuplicateGroup,
  DuplicatesResult,
  FileKind,
  FileRow,
  LargestResult,
  PlanCommand,
  PlanItem,
  SystemAction,
  SystemSinks,
} from "../types";

const MIB = 1024 ** 2;
const DAY = 86_400_000;
const gb = (n: number) => Math.round(n * GIB);
const mb = (n: number) => Math.round(n * MIB);

type Row = [name: string, dir: string, size: number, daysAgo: number, kind: FileKind];

let nextId = 90_000;
const rows = new Map<number, FileRow>();

function file([name, dir, size, daysAgo, kind]: Row, now: number, app = false): FileRow {
  const id = ++nextId;
  const when = now - daysAgo * DAY;
  const r: FileRow = { id, name, path: `${dir}\\${name}`, size, logical: size, modified: when, accessed: when, kind, app };
  rows.set(id, r);
  return r;
}

const DORMANT: Row[] = [
  ["Montage-mariage-final.mov", "~\\Videos\\Mariage Julie", gb(18.4), 640, "video"],
  ["Rushes-ceremonie.mp4", "~\\Videos\\Mariage Julie", gb(9.1), 640, "video"],
  ["Rushes-soiree.mp4", "~\\Videos\\Mariage Julie", gb(7.6), 641, "video"],
  ["Windows11_23H2_French_x64.iso", "~\\Downloads", gb(6.2), 420, "disk"],
  ["ubuntu-22.04.3-desktop-amd64.iso", "~\\Downloads", gb(4.7), 510, "disk"],
  ["Setup-Office-2019.exe", "~\\Downloads", mb(812), 900, "installer"],
  ["Archives-photos-2019.zip", "~\\Documents\\Archives", gb(3.9), 1200, "archive"],
  ["Scans-impots-2020.pdf", "~\\Documents\\Archives", mb(240), 1050, "document"],
  ["Podcast-brut-ep12.wav", "~\\Music\\Podcast", gb(1.3), 300, "audio"],
];

const LARGEST: Row[] = [
  ["Montage-mariage-final.mov", "~\\Videos\\Mariage Julie", gb(18.4), 640, "video"],
  ["Ubuntu-dev.vhdx", "~\\VMs", gb(14.2), 12, "disk"],
  ["Rushes-ceremonie.mp4", "~\\Videos\\Mariage Julie", gb(9.1), 640, "video"],
  ["Rushes-soiree.mp4", "~\\Videos\\Mariage Julie", gb(7.6), 641, "video"],
  ["Windows11_23H2_French_x64.iso", "~\\Downloads", gb(6.2), 420, "disk"],
  ["Lightroom-catalogue-2024.zip", "~\\Pictures\\Sauvegardes", gb(5.4), 95, "archive"],
  ["ubuntu-22.04.3-desktop-amd64.iso", "~\\Downloads", gb(4.7), 510, "disk"],
  ["Archives-photos-2019.zip", "~\\Documents\\Archives", gb(3.9), 1200, "archive"],
  ["Cours-video-react.mp4", "~\\Videos\\Formations", gb(2.8), 60, "video"],
  ["Panorama-Alpes.psd", "~\\Pictures\\Retouches", gb(1.9), 30, "image"],
  ["Podcast-brut-ep12.wav", "~\\Music\\Podcast", gb(1.3), 300, "audio"],
  ["Setup-Office-2019.exe", "~\\Downloads", mb(812), 900, "installer"],
];

const DUPLICATES: { size: number; kind: FileKind; copies: [name: string, dir: string, daysAgo: number][] }[] = [
  {
    size: gb(9.1),
    kind: "video",
    copies: [
      ["Rushes-ceremonie.mp4", "~\\Videos\\Mariage Julie", 640],
      ["Rushes-ceremonie (1).mp4", "~\\Downloads", 600],
      ["Rushes-ceremonie - copie.mp4", "~\\Desktop", 590],
    ],
  },
  {
    size: gb(4.7),
    kind: "disk",
    copies: [
      ["ubuntu-22.04.3-desktop-amd64.iso", "~\\Downloads", 510],
      ["ubuntu-22.04.3-desktop-amd64.iso", "~\\Documents\\ISO", 505],
    ],
  },
  {
    size: mb(640),
    kind: "archive",
    copies: [
      ["Photos-vacances-2022.zip", "~\\Documents", 700],
      ["Photos-vacances-2022 (2).zip", "~\\Downloads", 698],
    ],
  },
];

export function mockDormant(months: number, now: number): DormantResult {
  const files = DORMANT.filter((r) => r[3] >= months * 30.44).map((r) => file(r, now));
  const byDir = new Map<string, FileRow[]>();
  for (const f of files) {
    const dir = f.path.slice(0, f.path.lastIndexOf("\\"));
    byDir.set(dir, [...(byDir.get(dir) ?? []), f]);
  }
  const folders = [...byDir.entries()]
    .map(([path, list]) => ({ path, size: list.reduce((a, f) => a + f.size, 0), count: list.length, files: list.sort((a, b) => b.size - a.size) }))
    .sort((a, b) => b.size - a.size);
  return {
    months,
    basis: "lastAccess",
    lastAccessRaw: 0x8000_0002,
    total: files.reduce((a, f) => a + f.size, 0),
    count: files.length,
    folders,
  };
}

export function mockLargest(kind: FileKind | null, now: number): LargestResult {
  const all = LARGEST.map((r) => file(r, now));
  const kinds = new Map<FileKind, number>();
  for (const f of all) kinds.set(f.kind, (kinds.get(f.kind) ?? 0) + 1);
  return {
    files: all.filter((f) => kind === null || f.kind === kind),
    kinds: [...kinds.entries()].sort((a, b) => b[1] - a[1]),
  };
}

export function mockDuplicates(scanId: number, now: number): DuplicatesResult {
  const groups: DuplicateGroup[] = DUPLICATES.map((g, i) => {
    const members = g.copies.map(([name, dir, daysAgo]) => file([name, dir, g.size, daysAgo, g.kind], now));
    return { id: i, size: g.size, wasted: g.size * (members.length - 1), members, keep: members[0]?.id ?? 0 };
  });
  return {
    scanId,
    groups,
    moreGroups: 0,
    wasted: groups.reduce((a, g) => a + g.wasted, 0),
    stats: { candidates: 1840, read: 214, cached: 1626, skippedCloud: 37, skippedNotFrozen: 0, errors: 0, hardLinks: 12 },
  };
}

export function mockSinks(): SystemSinks {
  return {
    available: true,
    systemDrive: "C:",
    hiberfil: gb(12.7),
    pagefile: gb(8),
    swapfile: mb(256),
    hibernateEnabled: true,
    fastStartup: true,
    windowsOld: true,
    restorePointFrequencyMin: 1440,
    lastAccessRaw: 0x8000_0002,
    lastAccessUpdated: true,
    free: gb(73.8),
  };
}

const SYSTEM: Record<SystemAction, { line: string; fr: string; en: string; size: (s: SystemSinks) => number }> = {
  components: {
    line: "Dism.exe /Online /Cleanup-Image /StartComponentCleanup",
    fr: "Outil officiel : nettoie le magasin des composants de Windows (WinSxS)",
    en: "Official tool: cleans up the Windows component store (WinSxS)",
    size: () => 0,
  },
  hibernateOff: {
    line: "powercfg.exe /hibernate off",
    fr: "Désactive l'hibernation : plus de démarrage rapide ni de veille prolongée",
    en: "Turns hibernation off: no more fast startup or hibernate",
    size: (s) => s.hiberfil ?? 0,
  },
  hibernateReduced: {
    line: "powercfg.exe /hibernate /type reduced",
    fr: "Hibernation réduite : garde le démarrage rapide, plus de veille prolongée",
    en: "Reduced hibernation: keeps fast startup, no more hibernate",
    size: () => 0,
  },
  oldestRestorePoint: {
    line: "vssadmin.exe delete shadows /for=C: /oldest /quiet",
    fr: "Supprime uniquement le plus ancien point de restauration de C:",
    en: "Deletes only the oldest restore point of C:",
    size: () => 0,
  },
  windowsOld: {
    line: "cleanmgr.exe /d C:",
    fr: "Nettoyage de disque de Windows : cochez « Installation(s) précédente(s) de Windows »",
    en: "Windows Disk Cleanup: tick \"Previous Windows installation(s)\"",
    size: () => 0,
  },
};

/** Éléments et commandes d'un plan du nettoyage avancé (identifiants → lignes affichées). */
export function mockAdvancedPlan(request: Exclude<CleanRequest, { kind: "recos" }>, en: boolean): { items: PlanItem[]; commands: PlanCommand[] } {
  if (request.kind === "system") {
    const sinks = mockSinks();
    const commands: PlanCommand[] = [];
    if (request.restorePoint) {
      commands.push({
        line: "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand …",
        explanation: en ? "Creates a system restore point first (one per 24 h by default)" : "Crée d'abord un point de restauration du système (un par 24 h par défaut)",
        prerequisite: true,
        required: true,
        needsElevation: true,
        size: 0,
      });
    }
    for (const a of request.actions) {
      const c = SYSTEM[a];
      commands.push({ line: c.line, explanation: en ? c.en : c.fr, prerequisite: false, required: false, needsElevation: true, size: c.size(sinks) });
    }
    return { items: [], commands };
  }
  const name = { dormant: ["Fichiers dormants", "Dormant files"], largest: ["Gros fichiers", "Large files"], duplicates: ["Doublons", "Duplicates"] }[request.kind];
  const items = request.ids.flatMap((id) => {
    const r = rows.get(id);
    return r ? [{ path: r.path, size: r.size, directory: false, rule: "1", ruleName: (en ? name[1] : name[0]) ?? "" }] : [];
  });
  return { items, commands: [] };
}

/** Lignes de commande du mock, pour le journal. */
export function mockProgram(line: string): string {
  if (line.startsWith("powershell")) return "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe";
  const exe = line.split(" ")[0] ?? "";
  return exe.endsWith(".exe") ? `C:\\Windows\\System32\\${exe}` : "C:\\Program Files\\nodejs\\node.exe";
}
