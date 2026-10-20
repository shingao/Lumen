// Données factices du jalon 1, reprises à l'identique de design/lumen-mockup.html.
// Tailles en Go (base 1024) ; dates relatives à « maintenant ».

import type { Category, RecoStatus, Status } from "../types";

export interface L10n {
  fr: string;
  en: string;
}

const same = (s: string): L10n => ({ fr: s, en: s });

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** Heure du jour d'aujourd'hui (h, m), ou décalage depuis maintenant. */
export type When = { daysAgo: number; at?: [number, number] } | { msAgo: number };

export const at = (daysAgo: number, h: number, m: number): When => ({ daysAgo, at: [h, m] });
export const ago = (ms: number): When => ({ msAgo: ms });
export const days = (n: number): When => ago(n * DAY);
export { DAY, HOUR };

export interface MockTile {
  id: string;
  parent: string | null;
  category: Category;
  status: Status;
  /** Nom affiché sur la tuile. */
  name: L10n;
  /** Chemin relatif au dossier utilisateur. */
  path: L10n;
  size: number;
  files: number;
  last: When;
  reco: string | null;
  /** Gain affiché (Go) ; absent = rien à récupérer. */
  gain?: number;
  gainStatus?: RecoStatus;
  gainUpTo?: boolean;
  /** Surface de la tuile si elle diffère de la taille affichée (maquette : projet sans ses artefacts). */
  layoutSize?: number;
}

const t = (
  id: string,
  parent: string | null,
  category: Category,
  status: Status,
  name: L10n | string,
  path: L10n | string,
  size: number,
  files: number,
  last: When,
  reco: string | null,
  gain?: { size: number; status: RecoStatus; upTo?: boolean },
): MockTile => ({
  id,
  parent,
  category,
  status,
  name: typeof name === "string" ? same(name) : name,
  path: typeof path === "string" ? same(path) : path,
  size,
  files,
  last,
  reco,
  ...(gain ? { gain: gain.size, gainStatus: gain.status, gainUpTo: gain.upTo ?? false } : {}),
});

const safe = (size: number) => ({ size, status: "safe" as const });
const review = (size: number, upTo = false) => ({ size, status: "review" as const, upTo });

export const USER_ROOT = { volume: "C:", segments: ["Users", "Arthur"] };

// Les sous-tuiles d'un dossier sont celles de la maquette : leurs tailles s'additionnent exactement.
export const TILES: MockTile[] = [
  t("projets", null, "code", "keep", { fr: "Projets", en: "Projects" }, { fr: "Projets", en: "Projects" }, 38.0, 1_204_518, at(0, 11, 42), null, safe(12.3)),
  t("p_tsubaki", "projets", "code", "keep", "tsubaki-android", { fr: "Projets\\tsubaki-android", en: "Projects\\tsubaki-android" }, 12.4, 96_310, at(0, 11, 42), null),
  t("p_tsubaki_b", "projets", "code", "safe", "build", { fr: "Projets\\tsubaki-android\\app\\build", en: "Projects\\tsubaki-android\\app\\build" }, 4.6, 41_875, days(3), "build", safe(4.6)),
  t("p_hina", "projets", "code", "keep", "hinageshi-app", { fr: "Projets\\hinageshi-app", en: "Projects\\hinageshi-app" }, 9.8, 318_044, at(1, 22, 10), null),
  t("p_hina_b", "projets", "code", "safe", "node_modules · dist", { fr: "Projets\\hinageshi-app\\node_modules", en: "Projects\\hinageshi-app\\node_modules" }, 4.2, 297_512, at(1, 22, 10), "build", safe(4.2)),
  t("p_kai", "projets", "code", "keep", "kaiseki-api", { fr: "Projets\\kaiseki-api", en: "Projects\\kaiseki-api" }, 6.1, 203_377, days(6), null),
  t("p_kai_b", "projets", "code", "safe", ".next · node_modules", { fr: "Projets\\kaiseki-api\\.next", en: "Projects\\kaiseki-api\\.next" }, 3.5, 188_960, days(6), "build", safe(3.5)),
  t("p_old", "projets", "code", "keep", "_archives", { fr: "Projets\\_archives", en: "Projects\\_archives" }, 9.7, 586_787, days(240), null),
  t("p_old_nm", "projets", "code", "review", { fr: "node_modules inactifs", en: "Inactive node_modules" }, { fr: "Projets\\_archives\\*\\node_modules", en: "Projects\\_archives\\*\\node_modules" }, 6.6, 571_230, days(240), "oldnm", review(6.6)),

  t("videos", null, "media", "keep", { fr: "Vidéos", en: "Videos" }, { fr: "Vidéos", en: "Videos" }, 26.0, 1_284, days(14), null),
  t("v_cap", "videos", "media", "keep", "Captures", { fr: "Vidéos\\Captures", en: "Videos\\Captures" }, 14.2, 612, days(14), null),
  t("v_mont", "videos", "media", "keep", { fr: "Montages", en: "Edits" }, { fr: "Vidéos\\Montages", en: "Videos\\Edits" }, 11.8, 672, days(31), null),

  t("docker", null, "tools", "keep", "Docker", "AppData\\Local\\Docker", 31.5, 214, ago(HOUR), "docker", review(14.0, true)),
  t("d_used", "docker", "tools", "keep", { fr: "vhdx · utilisé", en: "vhdx · used" }, { fr: "AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx · utilisé", en: "AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx · used" }, 17.5, 1, ago(HOUR), null),
  t("d_free", "docker", "tools", "review", { fr: "vhdx · vide", en: "vhdx · empty" }, { fr: "AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx · espace vide", en: "AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx · empty space" }, 14.0, 1, ago(HOUR), "docker", review(14.0)),

  t("documents", null, "media", "keep", "Documents", "Documents", 18.4, 24_906, at(0, 9, 15), null),
  t("doc_trav", "documents", "media", "keep", { fr: "Travail", en: "Work" }, { fr: "Documents\\Travail", en: "Documents\\Work" }, 9.6, 14_220, at(0, 9, 15), null),
  t("doc_perso", "documents", "media", "keep", { fr: "Perso", en: "Personal" }, { fr: "Documents\\Perso", en: "Documents\\Personal" }, 5.7, 7_864, days(5), null),
  t("doc_adm", "documents", "media", "keep", { fr: "Administratif", en: "Paperwork" }, { fr: "Documents\\Administratif", en: "Documents\\Paperwork" }, 3.1, 2_822, days(21), null),

  t("images", null, "media", "keep", "Images", { fr: "Images", en: "Pictures" }, 12.8, 18_337, days(4), null),
  t("i_pel", "images", "media", "keep", { fr: "Pellicule", en: "Camera Roll" }, { fr: "Images\\Pellicule", en: "Pictures\\Camera Roll" }, 9.4, 11_902, days(4), null),
  t("i_cap", "images", "media", "keep", "Captures", { fr: "Images\\Captures d'écran", en: "Pictures\\Screenshots" }, 3.4, 6_435, at(1, 16, 20), null),

  t("android", null, "android", "review", "Android\\Sdk", "AppData\\Local\\Android\\Sdk", 17.2, 402_118, days(35), "android", review(17.2)),
  t("a_sys", "android", "android", "review", "system-images", "AppData\\Local\\Android\\Sdk\\system-images", 9.8, 1_806, days(35), "android"),
  t("a_plat", "android", "android", "review", "platforms", "AppData\\Local\\Android\\Sdk\\platforms + build-tools", 5.3, 362_940, days(35), "android"),
  t("a_emu", "android", "android", "review", "emulator", "AppData\\Local\\Android\\Sdk\\emulator", 2.1, 37_372, days(35), "android"),

  t("ollama", null, "ai", "review", ".ollama", ".ollama\\models", 16.7, 38, days(21), "ollama", review(16.7)),
  t("o_deep", "ollama", "ai", "review", "deepseek-coder", ".ollama\\models · deepseek-coder", 7.9, 12, days(21), "ollama"),
  t("o_llama", "ollama", "ai", "review", "llama3", ".ollama\\models · llama3", 4.7, 13, days(60), "ollama"),
  t("o_mis", "ollama", "ai", "review", "mistral", ".ollama\\models · mistral", 4.1, 13, days(120), "ollama"),

  t("npm", null, "packages", "safe", "npm-cache", "AppData\\Local\\npm-cache", 13.4, 412_380, days(2), "npm", safe(13.4)),
  t("gradle", null, "packages", "safe", ".gradle", ".gradle", 10.6, 286_409, days(3), "gradle", safe(10.6)),
  t("dl", null, "media", "keep", { fr: "Téléchargements", en: "Downloads" }, { fr: "Téléchargements", en: "Downloads" }, 9.5, 1_126, at(1, 18, 3), null),
  t("temp", null, "tools", "safe", "Temp", "AppData\\Local\\Temp", 4.1, 9_742, at(0, 14, 2), "temp", safe(4.1)),
  t("pip", null, "packages", "safe", "pip & uv", ".cache\\pip + AppData\\Local\\uv", 3.0, 51_220, days(9), "pip", safe(3.0)),
  t("autres", null, "tools", "keep", { fr: "Autres", en: "Other" }, ".vscode, .config, AppData\\Roaming…", 2.8, 84_511, ago(3 * HOUR), null),
  t("a_d3d", "autres", "tools", "safe", "D3DSCache", "AppData\\Local\\D3DSCache", 0.8, 2_304, days(4), "d3d", safe(0.8)),
];

// Le projet garde sa taille totale mais sa tuile laisse la place à ses artefacts (comme la maquette).
const LAYOUT: Record<string, number> = { p_tsubaki: 7.8, p_hina: 5.6, p_kai: 2.6, p_old: 3.1 };
for (const tile of TILES) {
  const layout = LAYOUT[tile.id];
  if (layout !== undefined) tile.layoutSize = layout;
}

export interface MockReco {
  id: string;
  status: RecoStatus;
  name: L10n;
  path: L10n;
  size: number;
  highlight: string[];
  explanation: L10n;
  meta: L10n;
  files: [L10n | string, number][];
  /** `§` marque l'endroit où s'insère -WhatIf. */
  script: (L10n | string)[];
}

export const RECOS: MockReco[] = [
  {
    id: "npm", status: "safe", name: { fr: "Cache npm", en: "npm cache" }, path: same("AppData\\Local\\npm-cache"), size: 13.4,
    highlight: ["npm"],
    explanation: {
      fr: "Le cache npm ne contient que des copies de paquets déjà publiés : npm les retélécharge à la demande.",
      en: "The npm cache only holds copies of published packages: npm downloads them again when needed.",
    },
    meta: { fr: "3 dossiers · 412 380 fichiers · dernier accès il y a 2 jours", en: "3 folders · 412,380 files · last accessed 2 days ago" },
    files: [["~\\AppData\\Local\\npm-cache\\_cacache\\content-v2", 12.6], ["~\\AppData\\Local\\npm-cache\\_cacache\\index-v5", 0.6], ["~\\AppData\\Local\\npm-cache\\_npx", 0.2]],
    script: [{ fr: "# Lumen · Cache npm · 13,4 Go", en: "# Lumen · npm cache · 13.4 GB" }, { fr: "# Outil officiel : vide le cache puis vérifie son intégrité", en: "# Official tool: clears the cache, then checks its integrity" }, "npm cache clean --force", "npm cache verify"],
  },
  {
    id: "build", status: "safe", name: { fr: "Artefacts de build (node_modules, .next, dist)", en: "Build artifacts (node_modules, .next, dist)" }, path: { fr: "~\\Projets\\*\\", en: "~\\Projects\\*\\" }, size: 12.3,
    highlight: ["projets", "p_tsubaki_b", "p_hina_b", "p_kai_b"],
    explanation: {
      fr: "Ces dossiers sont régénérés par npm, Next.js ou Gradle au prochain build : aucun fichier source n'est touché.",
      en: "npm, Next.js or Gradle rebuild these folders on the next build: no source file is touched.",
    },
    meta: { fr: "6 dossiers · 528 347 fichiers · 3 projets actifs", en: "6 folders · 528,347 files · 3 active projects" },
    files: [
      [{ fr: "~\\Projets\\tsubaki-android\\app\\build", en: "~\\Projects\\tsubaki-android\\app\\build" }, 3.1],
      [{ fr: "~\\Projets\\tsubaki-android\\build", en: "~\\Projects\\tsubaki-android\\build" }, 1.5],
      [{ fr: "~\\Projets\\hinageshi-app\\node_modules", en: "~\\Projects\\hinageshi-app\\node_modules" }, 3.4],
      [{ fr: "~\\Projets\\hinageshi-app\\dist", en: "~\\Projects\\hinageshi-app\\dist" }, 0.8],
      [{ fr: "~\\Projets\\kaiseki-api\\node_modules", en: "~\\Projects\\kaiseki-api\\node_modules" }, 2.6],
      [{ fr: "~\\Projets\\kaiseki-api\\.next", en: "~\\Projects\\kaiseki-api\\.next" }, 0.9],
    ],
    script: [
      { fr: "# Lumen · Artefacts de build · 12,3 Go", en: "# Lumen · Build artifacts · 12.3 GB" },
      { fr: "# Recréés au prochain npm install / gradle build", en: "# Recreated by the next npm install / gradle build" },
      { fr: "$racine = \"$env:USERPROFILE\\Projets\"", en: "$root = \"$env:USERPROFILE\\Projects\"" },
      { fr: "$cibles = @(", en: "$targets = @(" },
      "  \"tsubaki-android\\app\\build\",",
      "  \"tsubaki-android\\build\",",
      "  \"hinageshi-app\\node_modules\",",
      "  \"hinageshi-app\\dist\",",
      "  \"kaiseki-api\\node_modules\",",
      "  \"kaiseki-api\\.next\"",
      ")",
      { fr: "foreach ($c in $cibles) {", en: "foreach ($t in $targets) {" },
      { fr: "  $p = Join-Path $racine $c", en: "  $p = Join-Path $root $t" },
      "  if (Test-Path $p) { Remove-Item $p -Recurse -Force§ }",
      "}",
    ],
  },
  {
    id: "gradle", status: "safe", name: { fr: "Caches Gradle", en: "Gradle caches" }, path: same("~\\.gradle"), size: 10.6,
    highlight: ["gradle"],
    explanation: {
      fr: "Gradle recrée ses caches et retélécharge les dépendances au prochain build, qui sera simplement plus long.",
      en: "Gradle rebuilds its caches and downloads dependencies again on the next build, which will just take longer.",
    },
    meta: { fr: "3 dossiers · 286 409 fichiers · dernier build il y a 3 jours", en: "3 folders · 286,409 files · last build 3 days ago" },
    files: [["~\\.gradle\\caches\\modules-2", 6.1], ["~\\.gradle\\caches\\transforms-4", 2.8], ["~\\.gradle\\wrapper\\dists", 1.7]],
    script: [
      { fr: "# Lumen · Caches Gradle · 10,6 Go", en: "# Lumen · Gradle caches · 10.6 GB" },
      { fr: "# Arrête les démons Gradle avant de supprimer", en: "# Stops the Gradle daemons before deleting" },
      "gradle --stop",
      "$g = \"$env:USERPROFILE\\.gradle\"",
      "Remove-Item \"$g\\caches\" -Recurse -Force§",
      "Remove-Item \"$g\\wrapper\\dists\" -Recurse -Force§",
    ],
  },
  {
    id: "temp", status: "safe", name: { fr: "Fichiers temporaires", en: "Temporary files" }, path: same("AppData\\Local\\Temp"), size: 4.1,
    highlight: ["temp"],
    explanation: {
      fr: "Fichiers laissés par des installations et des applis ; ceux en cours d'utilisation sont ignorés.",
      en: "Files left behind by installers and apps; the ones in use are skipped.",
    },
    meta: { fr: "9 742 fichiers · plus anciens que 48 h", en: "9,742 files · older than 48 h" },
    files: [
      ["~\\AppData\\Local\\Temp\\*.tmp", 2.2],
      [{ fr: "~\\AppData\\Local\\Temp\\ (14 dossiers d'installeurs)", en: "~\\AppData\\Local\\Temp\\ (14 installer folders)" }, 1.4],
      ["~\\AppData\\Local\\Temp\\*.log, *.dmp", 0.5],
    ],
    script: [
      { fr: "# Lumen · Fichiers temporaires · 4,1 Go", en: "# Lumen · Temporary files · 4.1 GB" },
      { fr: "# Ignore les fichiers récents (< 48 h) ou verrouillés", en: "# Skips recent (< 48 h) or locked files" },
      { fr: "$limite = (Get-Date).AddDays(-2)", en: "$limit = (Get-Date).AddDays(-2)" },
      "Get-ChildItem $env:TEMP -Force |",
      { fr: "  Where-Object { $_.LastWriteTime -lt $limite } |", en: "  Where-Object { $_.LastWriteTime -lt $limit } |" },
      "  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue§",
    ],
  },
  {
    id: "pip", status: "safe", name: { fr: "Caches pip & uv", en: "pip & uv caches" }, path: same("~\\.cache\\pip, ~\\AppData\\Local\\uv"), size: 3.0,
    highlight: ["pip"],
    explanation: {
      fr: "Caches de téléchargement de pip et uv, vidés proprement par leurs propres commandes.",
      en: "Download caches of pip and uv, cleared cleanly by their own commands.",
    },
    meta: { fr: "2 caches · 51 220 fichiers", en: "2 caches · 51,220 files" },
    files: [["~\\.cache\\pip\\http-v2", 1.8], ["~\\AppData\\Local\\uv\\cache", 1.2]],
    script: [{ fr: "# Lumen · Caches pip & uv · 3,0 Go", en: "# Lumen · pip & uv caches · 3.0 GB" }, "pip cache purge", "uv cache clean"],
  },
  {
    id: "d3d", status: "safe", name: { fr: "Cache de shaders DirectX", en: "DirectX shader cache" }, path: same("AppData\\Local\\D3DSCache"), size: 0.8,
    highlight: ["autres", "a_d3d"],
    explanation: {
      fr: "Windows reconstruit ce cache au lancement des jeux, au prix de quelques saccades la première fois.",
      en: "Windows rebuilds this cache when games start, with a little stutter the first time.",
    },
    meta: { fr: "2 304 fichiers · reconstruit automatiquement", en: "2,304 files · rebuilt automatically" },
    files: [["~\\AppData\\Local\\D3DSCache", 0.8]],
    script: [
      { fr: "# Lumen · Cache de shaders DirectX · 0,8 Go", en: "# Lumen · DirectX shader cache · 0.8 GB" },
      { fr: "# Windows le reconstruit au prochain lancement des jeux", en: "# Windows rebuilds it the next time games start" },
      "Remove-Item \"$env:LOCALAPPDATA\\D3DSCache\\*\" -Recurse -Force§",
    ],
  },
  {
    id: "android", status: "review", name: { fr: "Android SDK & émulateurs", en: "Android SDK & emulators" }, path: same("AppData\\Local\\Android\\Sdk"), size: 17.2,
    highlight: ["android", "a_sys", "a_plat", "a_emu"],
    explanation: {
      fr: "Images système et plateformes peut-être inutiles ; vérifie qu'aucun projet actif ne cible encore ces API.",
      en: "System images and platforms you may not need; check that no active project still targets these APIs.",
    },
    meta: { fr: "2 images système · 5 plateformes · 6 build-tools", en: "2 system images · 5 platforms · 6 build-tools" },
    files: [
      ["~\\AppData\\Local\\Android\\Sdk\\system-images\\android-33", 5.1],
      ["~\\AppData\\Local\\Android\\Sdk\\system-images\\android-34", 4.7],
      [{ fr: "~\\AppData\\Local\\Android\\Sdk\\platforms (5 versions)", en: "~\\AppData\\Local\\Android\\Sdk\\platforms (5 versions)" }, 3.6],
      ["~\\AppData\\Local\\Android\\Sdk\\emulator", 2.1],
      [{ fr: "~\\AppData\\Local\\Android\\Sdk\\build-tools (6 versions)", en: "~\\AppData\\Local\\Android\\Sdk\\build-tools (6 versions)" }, 1.7],
    ],
    script: [
      { fr: "# Lumen · Android SDK & émulateurs · 17,2 Go", en: "# Lumen · Android SDK & emulators · 17.2 GB" },
      { fr: "# À vérifier : garde les API ciblées par tes projets actifs", en: "# To review: keep the APIs your active projects target" },
      "sdkmanager --uninstall \"system-images;android-33;google_apis;x86_64\"",
      "sdkmanager --uninstall \"system-images;android-34;google_apis;x86_64\"",
      "sdkmanager --uninstall \"platforms;android-31\" \"platforms;android-32\"",
    ],
  },
  {
    id: "ollama", status: "review", name: { fr: "Modèles Ollama", en: "Ollama models" }, path: same("~\\.ollama\\models"), size: 16.7,
    highlight: ["ollama", "o_deep", "o_llama", "o_mis"],
    explanation: {
      fr: "Modèles locaux téléchargés pour Ollama ; ils se retéléchargent avec ollama pull si tu en as de nouveau besoin.",
      en: "Local models downloaded for Ollama; ollama pull downloads them again if you need them later.",
    },
    meta: { fr: "3 modèles · dernier usage il y a 3 semaines", en: "3 models · last used 3 weeks ago" },
    files: [["~\\.ollama\\models · deepseek-coder", 7.9], ["~\\.ollama\\models · llama3", 4.7], ["~\\.ollama\\models · mistral", 4.1]],
    script: [
      { fr: "# Lumen · Modèles Ollama · 16,7 Go", en: "# Lumen · Ollama models · 16.7 GB" },
      { fr: "# Retéléchargeables avec : ollama pull <modèle>", en: "# Download again with: ollama pull <model>" },
      "ollama rm deepseek-coder",
      "ollama rm llama3",
      "ollama rm mistral",
    ],
  },
  {
    id: "docker", status: "review", name: { fr: "Disque Docker / WSL (compactage)", en: "Docker / WSL disk (compaction)" }, path: same("AppData\\Local\\Docker\\wsl\\disk"), size: 14.0,
    highlight: ["docker", "d_free"],
    explanation: {
      fr: "Le disque virtuel WSL ne rétrécit jamais seul : le compactage rend l'espace vide sans toucher à tes images ni à tes volumes.",
      en: "The WSL virtual disk never shrinks on its own: compaction gives back the empty space without touching your images or volumes.",
    },
    meta: { fr: "1 disque virtuel · 31,5 Go alloués · 17,5 Go utilisés", en: "1 virtual disk · 31.5 GB allocated · 17.5 GB used" },
    files: [[{ fr: "~\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx (espace vide)", en: "~\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx (empty space)" }, 14.0]],
    script: [
      { fr: "# Lumen · Compactage Docker / WSL · 14,0 Go", en: "# Lumen · Docker / WSL compaction · 14.0 GB" },
      { fr: "# Quitte Docker Desktop, puis lance en administrateur", en: "# Quit Docker Desktop, then run as administrator" },
      "wsl --shutdown",
      "Optimize-VHD -Path \"$env:LOCALAPPDATA\\Docker\\wsl\\disk\\docker_data.vhdx\" -Mode Full",
    ],
  },
  {
    id: "oldnm", status: "review", name: { fr: "node_modules inactifs (+ de 6 mois)", en: "Inactive node_modules (6+ months)" }, path: { fr: "~\\Projets\\_archives\\*\\node_modules", en: "~\\Projects\\_archives\\*\\node_modules" }, size: 6.6,
    highlight: ["projets", "p_old_nm"],
    explanation: {
      fr: "Dépendances de projets sans commit depuis plus de 6 mois : un npm install les recrée si tu y reviens.",
      en: "Dependencies of projects with no commit for over 6 months: npm install recreates them if you come back.",
    },
    meta: { fr: "3 projets · dernier commit il y a 8 mois", en: "3 projects · last commit 8 months ago" },
    files: [
      [{ fr: "~\\Projets\\_archives\\portfolio-2024\\node_modules", en: "~\\Projects\\_archives\\portfolio-2024\\node_modules" }, 2.4],
      [{ fr: "~\\Projets\\_archives\\tp-react\\node_modules", en: "~\\Projects\\_archives\\tp-react\\node_modules" }, 2.3],
      [{ fr: "~\\Projets\\_archives\\bot-discord\\node_modules", en: "~\\Projects\\_archives\\bot-discord\\node_modules" }, 1.9],
    ],
    script: [
      { fr: "# Lumen · node_modules inactifs · 6,6 Go", en: "# Lumen · Inactive node_modules · 6.6 GB" },
      { fr: "$racine = \"$env:USERPROFILE\\Projets\\_archives\"", en: "$root = \"$env:USERPROFILE\\Projects\\_archives\"" },
      { fr: "Get-ChildItem $racine -Directory | ForEach-Object {", en: "Get-ChildItem $root -Directory | ForEach-Object {" },
      "  $nm = Join-Path $_.FullName \"node_modules\"",
      "  if (Test-Path $nm) { Remove-Item $nm -Recurse -Force§ }",
      "}",
    ],
  },
  {
    id: "winold", status: "review", name: { fr: "Windows.old (outil Windows officiel)", en: "Windows.old (official Windows tool)" }, path: same("C:\\Windows.old"), size: 7.0,
    highlight: [],
    explanation: {
      fr: "Copie de ton ancienne installation de Windows, gardée pour revenir en arrière : à supprimer uniquement via l'outil Windows.",
      en: "A copy of your previous Windows installation, kept to roll back: remove it only with the Windows tool.",
    },
    meta: { fr: "Installation précédente du 12/06/2026", en: "Previous installation from 06/12/2026" },
    files: [["C:\\Windows.old", 7.0]],
    script: [
      { fr: "# Lumen · Windows.old · 7,0 Go", en: "# Lumen · Windows.old · 7.0 GB" },
      { fr: "# Passe par l'outil officiel, jamais par une suppression manuelle", en: "# Goes through the official tool, never a manual deletion" },
      "Start-Process cleanmgr.exe -ArgumentList \"/d C:\" -Verb RunAs",
      { fr: "# Puis : Nettoyer les fichiers système ›", en: "# Then: Clean up system files ›" },
      { fr: "#   Installation(s) précédente(s) de Windows", en: "#   Previous Windows installation(s)" },
    ],
  },
];

/** Cochés par défaut : uniquement des éléments sans risque (un « À vérifier » ne l'est jamais). */
export const DEFAULT_CHECKED = ["npm", "build", "gradle", "temp", "pip"];

/** KPI de la maquette (Go). */
export const KPI = {
  freeNow: 73.8,
  freeAfter: 118,
  unanalyzed: 142,
  trend7d: 18,
};

// Disque D: (données) : un petit jeu de tuiles pour montrer le sélecteur et la catégorie Jeux.
export const D_TILES: MockTile[] = [
  t("steam", null, "games", "keep", "SteamLibrary", "SteamLibrary", 412, 98_204, days(2), null),
  t("s_cyber", "steam", "games", "keep", "Cyberpunk 2077", "SteamLibrary\\steamapps\\common\\Cyberpunk 2077", 118, 21_118, days(2), null),
  t("s_bg3", "steam", "games", "review", "Baldur's Gate 3", "SteamLibrary\\steamapps\\common\\Baldurs Gate 3", 96, 18_442, days(240), null),
  t("s_ff", "steam", "games", "keep", "Final Fantasy XVI", "SteamLibrary\\steamapps\\common\\FINAL FANTASY XVI", 112, 9_877, days(12), null),
  t("s_other", "steam", "games", "keep", { fr: "12 autres jeux", en: "12 other games" }, "SteamLibrary\\steamapps\\common", 86, 48_767, days(30), null),
  t("rushes", null, "media", "keep", "Rushes", "Rushes", 268, 3_402, days(45), null),
  t("r_2025", "rushes", "media", "keep", "2025", "Rushes\\2025", 171, 2_102, days(200), null),
  t("r_2026", "rushes", "media", "keep", "2026", "Rushes\\2026", 97, 1_300, days(45), null),
  t("backup", null, "media", "keep", "Backups", "Backups", 143, 1_046_337, days(400), null),
  t("vms", null, "tools", "keep", "VMs", "VMs", 64, 18, days(90), null),
];

export const VOLUMES = [
  { id: "C:", letter: "C:", label: { fr: "Système", en: "System" }, kind: "fixed" as const, total: 476, free: 73.8 },
  { id: "D:", letter: "D:", label: { fr: "Données", en: "Data" }, kind: "fixed" as const, total: 931, free: 44 },
  { id: "\\\\NAS\\partage", letter: "\\\\NAS\\partage", label: { fr: "Réseau", en: "Network" }, kind: "network" as const, total: 3726, free: 1210 },
];
