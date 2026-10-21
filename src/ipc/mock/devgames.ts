// Jeux et Dev du backend factice (jalon 8) : launchers, caches de shaders, projets, Docker, WSL.

import { GIB } from "../../core/format";
import type { DevCache, DevProject, DockerImages, Game, GamesResult, PlanCommand, PlanItem, ShaderCaches, WslDisks } from "../types";

const DAY = 86_400_000;
const gb = (n: number) => Math.round(n * GIB);

/** Coffre factice : `localStorage` `lumen-dev-running` = jeu ou launcher ouvert (ex. `steam.exe`). */
export function mockRunning(): string[] {
  try {
    const v = globalThis.localStorage.getItem("lumen-dev-running");
    return v ? v.split(",").map((s) => s.trim()).filter(Boolean) : [];
  } catch {
    return [];
  }
}

/** Coffre factice : `localStorage` `lumen-dev-docker` = `off` (arrêté) ou `absent` (non installé). */
function dockerState(): "on" | "off" | "absent" {
  try {
    const v = globalThis.localStorage.getItem("lumen-dev-docker");
    return v === "off" || v === "absent" ? v : "on";
  } catch {
    return "on";
  }
}

type MockGame = Omit<Game, "lastPlayed" | "status" | "size"> & { size: number | null; daysAgo: number | null; hue: number };

const GAMES: MockGame[] = [
  { id: "steam:1086940", launcher: "steam", name: "Baldur's Gate 3", installDir: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Baldurs Gate 3", size: 149.8, sizeSource: "launcher", drive: "C:", daysAgo: 3, cover: true, open: "uri", hue: 18 },
  { id: "steam:1091500", launcher: "steam", name: "Cyberpunk 2077", installDir: "D:\\SteamLibrary\\steamapps\\common\\Cyberpunk 2077", size: 71.4, sizeSource: "launcher", drive: "D:", daysAgo: 290, cover: true, open: "uri", hue: 55 },
  { id: "steam:730", launcher: "steam", name: "Counter-Strike 2", installDir: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Counter-Strike Global Offensive", size: 33.2, sizeSource: "launcher", drive: "C:", daysAgo: 1, cover: true, open: "uri", hue: 210 },
  { id: "steam:1245620", launcher: "steam", name: "Elden Ring", installDir: "D:\\SteamLibrary\\steamapps\\common\\ELDEN RING", size: 49.1, sizeSource: "launcher", drive: "D:", daysAgo: 410, cover: false, open: "uri", hue: 40 },
  { id: "epic:Fortnite", launcher: "epic", name: "Fortnite", installDir: "D:\\Epic Games\\Fortnite", size: 38.6, sizeSource: "launcher", drive: "D:", daysAgo: null, cover: false, open: "app", hue: 280 },
  { id: "gog:1207664663", launcher: "gog", name: "The Witcher 3: Wild Hunt", installDir: "C:\\GOG Games\\The Witcher 3 Wild Hunt GOTY", size: null, sizeSource: "measured", drive: "C:", daysAgo: null, cover: false, open: "app", hue: 0 },
  { id: "battlenet:Fen", launcher: "battlenet", name: "Diablo IV", installDir: "D:\\Battle.net\\Diablo IV", size: null, sizeSource: "measured", drive: "D:", daysAgo: null, cover: false, open: "app", hue: 0 },
];

const MEASURED: Record<string, number> = { "gog:1207664663": 46.3, "battlenet:Fen": 88.9 };

export function mockGames(now: number): GamesResult {
  const games: Game[] = GAMES.map(({ daysAgo, hue: _hue, size, ...g }) => {
    const lastPlayed = daysAgo === null ? null : now - daysAgo * DAY;
    return {
      ...g,
      size: gb(size ?? MEASURED[g.id] ?? 0),
      lastPlayed,
      status: lastPlayed !== null && now - lastPlayed > 183 * DAY ? "review" : "keep",
    };
  });
  return {
    games,
    ignored: [
      { launcher: "steam", file: "D:\\SteamLibrary\\steamapps\\appmanifest_292030.acf", reason: "malformed" },
      { launcher: "epic", file: "C:\\ProgramData\\Epic\\EpicGamesLauncher\\Data\\Manifests\\A1B2C3.item", reason: "incomplete" },
    ],
    launchers: ["steam", "epic", "gog", "battlenet"],
    steamShaderCaches: [
      { path: "C:\\Program Files (x86)\\Steam\\steamapps\\shadercache", size: gb(2.4) },
      { path: "D:\\SteamLibrary\\steamapps\\shadercache", size: gb(0.9) },
    ],
  };
}

/** Jaquette factice : un aplat dégradé (le vrai backend lit le cache local du launcher). */
export function mockCover(id: string): string | null {
  const g = GAMES.find((x) => x.id === id);
  if (!g?.cover) return null;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="300" height="450" viewBox="0 0 300 450">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl(${g.hue} 45% 62%)"/><stop offset="1" stop-color="hsl(${g.hue + 30} 40% 28%)"/></linearGradient></defs>` +
    `<rect width="300" height="450" fill="url(#g)"/></svg>`;
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}

const SHADERS = [
  { path: "C:\\Users\\Arthur\\AppData\\Local\\NVIDIA\\DXCache", size: 1.8, files: 412 },
  { path: "C:\\Users\\Arthur\\AppData\\Local\\NVIDIA\\GLCache", size: 0.3, files: 96 },
  { path: "C:\\Users\\Arthur\\AppData\\Local\\D3DSCache", size: 0.65, files: 1_204 },
];

export function mockShaders(scanId: number | null): ShaderCaches {
  if (scanId === null) return { analysed: false, caches: [], total: 0 };
  const caches = SHADERS.map((c) => ({ path: c.path, size: gb(c.size), files: c.files }));
  return { analysed: true, caches, total: caches.reduce((a, c) => a + c.size, 0) };
}

type MockProject = Omit<DevProject, "lastActivity" | "sourceSize" | "gitSize" | "artifactsSize" | "artifacts"> & {
  daysAgo: number;
  source: number;
  gitMb: number;
  artifacts: [number, string, number, "safe" | "review"][];
};

const BASE = "C:\\Users\\Arthur\\Projets\\";

const PROJECTS: MockProject[] = [
  { id: 9100, name: "hinageshi-web", path: `${BASE}hinageshi-web`, language: "TypeScript", daysAgo: 2, activitySource: "git", git: true, lockfile: true, source: 0.048, gitMb: 120, artifacts: [[9101, "node_modules", 1.2, "safe"], [9102, ".next", 0.34, "safe"]] },
  { id: 9110, name: "kaiju-engine", path: `${BASE}kaiju-engine`, language: "Rust", daysAgo: 400, activitySource: "git", git: true, lockfile: true, source: 0.012, gitMb: 46, artifacts: [[9111, "target", 8.4, "review"]] },
  { id: 9120, name: "notebooks-meteo", path: `${BASE}notebooks-meteo`, language: "Python", daysAgo: 220, activitySource: "files", git: false, lockfile: true, source: 0.31, gitMb: 0, artifacts: [[9121, ".venv", 2.1, "review"]] },
  { id: 9130, name: "carnet-android", path: `${BASE}carnet-android`, language: "Kotlin", daysAgo: 30, activitySource: "git", git: true, lockfile: false, source: 0.022, gitMb: 18, artifacts: [[9131, "app\\build", 1.6, "safe"], [9132, ".gradle", 0.4, "safe"]] },
  { id: 9140, name: "api-meteo", path: `${BASE}api-meteo`, language: "Go", daysAgo: 12, activitySource: "git", git: true, lockfile: true, source: 0.004, gitMb: 3, artifacts: [] },
  { id: 9150, name: "prototype-canvas", path: `${BASE}prototype-canvas`, language: "JavaScript", daysAgo: 95, activitySource: "files", git: false, lockfile: false, source: 0.002, gitMb: 0, artifacts: [] },
];

export function mockProjects(now: number): DevProject[] {
  return PROJECTS.map(({ daysAgo, source, gitMb, artifacts, ...p }) => {
    const list = artifacts.map(([id, path, size, status]) => ({ id, path, size: gb(size), status }));
    return { ...p, lastActivity: now - daysAgo * DAY, sourceSize: gb(source), gitSize: Math.round(gitMb * 1024 * 1024), artifactsSize: list.reduce((a, x) => a + x.size, 0), artifacts: list };
  });
}

/** Caches globaux hors des recommandations de la maquette (classement du jalon 4). */
export const DEV_EXTRA_CACHES: { id: string; status: "safe" | "review"; fr: string; en: string; path: string; size: number; command: boolean; line?: string }[] = [
  { id: "nuget", status: "safe", fr: "Cache NuGet", en: "NuGet cache", path: "~\\.nuget\\packages", size: 2.2, command: true, line: "dotnet nuget locals all --clear" },
  { id: "cargo", status: "review", fr: "Registre Cargo", en: "Cargo registry", path: "~\\.cargo\\registry", size: 3.4, command: false },
  { id: "m2", status: "review", fr: "Dépôt Maven local (.m2)", en: "Local Maven repository (.m2)", path: "~\\.m2\\repository", size: 1.9, command: false },
];

export function mockDevCaches(base: DevCache[], en: boolean): DevCache[] {
  return [...base, ...DEV_EXTRA_CACHES.map((c) => ({ id: c.id, status: c.status, name: en ? c.en : c.fr, path: c.path, size: gb(c.size), command: c.command }))];
}

export function mockDocker(): DockerImages {
  const state = dockerState();
  if (state !== "on") return { running: false, reason: state === "absent" ? "notInstalled" : "notRunning", images: [], total: 0 };
  const images = [
    { id: "sha256:4f2b9c1e", repository: "<none>", tag: "<none>", size: 1_210_000_000, created: "3 weeks ago" },
    { id: "sha256:a91c07d2", repository: "<none>", tag: "<none>", size: 523_000_000, created: "2 months ago" },
    { id: "sha256:07e5f3aa", repository: "<none>", tag: "<none>", size: 88_400_000, created: "5 months ago" },
  ];
  return { running: true, reason: null, images, total: images.reduce((a, i) => a + i.size, 0) };
}

const WSL = [
  { id: 0, name: "Ubuntu", kind: "wsl" as const, path: "C:\\Users\\Arthur\\AppData\\Local\\Packages\\CanonicalGroupLimited.Ubuntu_79rhkp1fndgsc\\LocalState\\ext4.vhdx", size: 21.6 },
  { id: 1, name: "Debian", kind: "wsl" as const, path: "D:\\WSL\\Debian\\ext4.vhdx", size: 6.2 },
  { id: 2, name: "Docker Desktop (données)", kind: "docker" as const, path: "C:\\Users\\Arthur\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx", size: 34.8 },
];

export function mockWsl(): WslDisks {
  return { disks: WSL.map((d) => ({ ...d, size: gb(d.size) })), hyperv: false };
}

/** Plans du jalon 8 (identifiants → lignes affichées). */
export function mockDevGamesPlan(
  request: { kind: "shaders" } | { kind: "artifacts"; ids: number[] } | { kind: "dockerPrune" } | { kind: "wslCompact"; ids: number[] },
  en: boolean,
  now: number,
): { items: PlanItem[]; commands: PlanCommand[] } {
  switch (request.kind) {
    case "shaders": {
      const name = en ? "Shader caches" : "Caches de shaders";
      return { items: SHADERS.map((c) => ({ path: c.path, size: gb(c.size), directory: true, rule: "caches-shaders", ruleName: name })), commands: [] };
    }
    case "artifacts": {
      const items = mockProjects(now).flatMap((p) =>
        p.artifacts
          .filter((a) => request.ids.includes(a.id))
          .map((a) => ({
            path: `${p.path}\\${a.path}`,
            size: a.size,
            directory: true,
            rule: a.status === "safe" ? "artefacts-projets-actifs" : "artefacts-projets-inactifs",
            ruleName: a.status === "safe" ? (en ? "Build artifacts" : "Artefacts de build") : en ? "Artifacts of inactive projects" : "Artefacts de projets inactifs",
          })),
      );
      return { items, commands: [] };
    }
    case "dockerPrune": {
      const d = mockDocker();
      return {
        items: [],
        commands: [{ line: "docker.exe image prune --force", explanation: en ? "Official tool: removes the untagged images listed above" : "Outil officiel : supprime les images sans étiquette listées", prerequisite: false, required: false, needsElevation: false, size: d.total }],
      };
    }
    case "wslCompact": {
      const disks = mockWsl().disks.filter((d) => request.ids.includes(d.id));
      const commands: PlanCommand[] = [
        { line: "wsl.exe --shutdown", explanation: en ? "Stops WSL: every Linux and Docker process stops" : "Arrête WSL : tous les processus Linux et Docker s'arrêtent", prerequisite: true, required: true, needsElevation: false, size: 0 },
        ...disks.map((d) => ({
          line: "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand …",
          explanation: en ? `Compacts ${d.name} (diskpart, attached read-only): ${d.path}` : `Compacte ${d.name} (diskpart, attaché en lecture seule) : ${d.path}`,
          prerequisite: false,
          required: false,
          needsElevation: true,
          // Gain mesuré après coup (espace libre avant / après), comme le vrai backend.
          size: 0,
        })),
      ];
      return { items: [], commands };
    }
  }
}
