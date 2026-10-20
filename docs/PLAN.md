# Lumen — plan d'architecture (validé)

Appli desktop Windows 11 qui explique pourquoi le disque est plein et aide à le libérer sans risque.
Plan validé le 20/10/2026 avec les décisions du § 10.

Maquette de référence : `design/lumen-mockup.html` (écran principal + panneau de détail), relue en entier,
rendue dans Chromium (captures `design/maquettes/01-principal.png`, `02-detail.png`).

---

## 0. Ce que j'ai relevé dans la maquette

Elle sert de source de vérité. Points qui complètent ou précisent le brief :

| Élément | Maquette | Conséquence |
|---|---|---|
| Barre de titre | 44 px, logo 18 px + « Lumen » 13 px/600, recherche 420 px max / 28 px de haut / rayon 8, `kbd` mono 10,5 px, boutons fenêtre 46 px | On ajoute soleil/lune + Paramètres juste avant les boutons fenêtre, mêmes boutons fantômes 46 px |
| Contrôle segmenté | fond `#F5F5F4`, bord, rayon 9, padding 3, boutons rayon 6 / 5×12 px / 12 px/500, actif = blanc + ombre légère | Réutilisé pour la navigation principale (Analyse, Timeline…), la bascule Catégorie / Nettoyage / **Âge** (Âge absent de la maquette, ajouté), et les onglets |
| KPI | une seule carte rayon 16, cellules séparées par des filets de 1 px (`gap: 1px` sur fond bordure), libellé 12 px, valeur mono 26 px/500, sous-ligne mono 11,5 px | « +18 GB / 7 j » vient dans la sous-ligne, en lien vers la Timeline |
| Conteneur principal | Explorer (flex 999) + colonne droite 400 px séparée par un filet | Le panneau de détail **remplace** la liste dans la colonne droite (retour « ‹ Recommandations ») |
| Tuiles | rayon 6 (niveau 1) et 5 (niveau 2), gap 3, padding 8×9 / 5×7, nom 12/600 puis 11/500, taille mono 10,5, delta vert `#166534` (« −12,3 GB ») ou ambre `#92400E` (« −16,7 GB ? ») | Delta = gain possible ; « ? » = à vérifier |
| Survol | les autres tuiles passent en opacité réduite, anneau sombre 1,5 px sur la tuile | Même mécanisme pour le survol d'une recommandation |
| Infobulle | 268 px, rayon 10, chemin mono 11 px, grille 2 colonnes (Taille, Fichiers, Dernière ouverture, Statut), ombre `0 6px 20px rgba(28,25,23,.08)` | Ce style devient aussi celui des astuces contextuelles et des menus |
| Bouton « Nettoyer » | **vert `#15803D`**, pas le noir primaire | Validé : vert dans les deux thèmes (`--btn-clean-bg`) ; « Copier le script » noir, inversé en sombre |
| Neutres supplémentaires | `#292524`, `#44403C`, `#57534E`, `#A8A29E`, `#D6D3D1`, `#F0EEEC`, `#F5F5F4` (échelle Stone) | Deviennent `--text-2/3/4`, `--border-soft`, `--surface-sunken`… avec une transposition sombre (voir § 4) |
| Unités | « GB » et virgule décimale (`13,4 GB`, `412 380`) | Décision : « Go » en français (`13,4 Go`), « GB » en anglais, base 1024 (§ 10) |

---

## 1. Emplacement du code

Dépôt dédié `shingao/lumen` (décision du 20/10/2026). L'arborescence ci-dessous est celle de sa racine.

---

## 2. Arborescence

```
lumen/
├─ CLAUDE.md                     # tokens 2 thèmes, règles de sécurité, structure, commandes
├─ PROGRESS.md                   # jalons : fait / reste / décisions / écarts maquette
├─ design/
│  ├─ lumen-mockup.html          # maquette (source de vérité visuelle)
│  └─ maquettes/                 # captures de référence + captures de chaque jalon
├─ package.json, vite.config.ts, tsconfig*.json, eslint.config.js, .stylelintrc.json
├─ index.html
├─ public/theme-boot.js          # pose data-theme avant le premier rendu (pas de flash)
├─ src/                          # front React 19 + TS strict, CSS fait main
│  ├─ main.tsx
│  ├─ app/                       # App, store (zustand), navigation, raccourcis, commandes palette
│  ├─ ipc/
│  │  ├─ bindings.ts             # GÉNÉRÉ par tauri-specta depuis les types Rust (jamais édité)
│  │  ├─ client.ts               # seul endroit qui appelle invoke()/listen()
│  │  └─ mock/                   # backend factice (chiffres de la maquette) pour `npm run dev` et les tests
│  ├─ components/                # Panel, Segmented, Button, Checkbox, Toggle, Select, Slider, Menu,
│  │                             # ContextMenu, Tooltip, Dialog, Kbd, StatusDot, ProportionBar, Mono, Hatch
│  ├─ features/
│  │  ├─ titlebar/  kpi/  explorer/ (treemap, fil d'Ariane, légende, infobulle, menu contextuel)
│  │  ├─ recommendations/  detail/  timeline/  cleanup/ (dormants, doublons, gros fichiers, gouffres)
│  │  ├─ dev/  games/  journal/  settings/  palette/  onboarding/  tips/  move/  report/
│  ├─ core/                      # logique pure, testée sans React
│  │  ├─ treemap/squarify.ts     # squarified maison, 2 niveaux, gap/padding en entrée
│  │  ├─ format.ts               # tailles (13,4 GB), nombres (412 380), dates relatives
│  │  ├─ ageScale.ts             # dégradé neutre du mode Âge
│  │  └─ contrast.ts             # ratio WCAG (utilisé par les tests de contraste)
│  ├─ i18n/fr.ts                 # toutes les chaînes visibles
│  ├─ styles/ tokens.css, fonts.css, global.css
│  └─ assets/fonts/              # Inter + JetBrains Mono woff2 (via @fontsource, pas de CDN)
├─ src-tauri/
│  ├─ Cargo.toml, tauri.conf.json, build.rs, clippy.toml
│  ├─ capabilities/main.json     # minimum : fenêtre (drag, min, max, close), events ; rien d'autre
│  ├─ rules/rules.toml           # moteur de règles en données (embarqué via include_str!)
│  ├─ migrations/0001_init.sql
│  ├─ src/
│  │  ├─ main.rs, lib.rs         # builder Tauri, plugins, export des bindings TS (debug)
│  │  ├─ error.rs                # LumenError (thiserror) → message utilisateur en français
│  │  ├─ state.rs                # AppState : db, scans en mémoire, jobs, plans
│  │  ├─ ipc/                    # commandes fines par domaine : valident, délèguent, ne touchent jamais le disque
│  │  ├─ db/                     # rusqlite, migrations, pool (1 écrivain + lecteurs)
│  │  ├─ scan/
│  │  │  ├─ model.rs             # arène de nœuds (voir § 4)
│  │  │  ├─ walk.rs              # jwalk parallèle
│  │  │  ├─ mft/                 # lecture brute $MFT (élevé) : reader, record, attributes
│  │  │  ├─ volumes.rs           # C:, D:, réseau ; type, FS, libre/total, MFT possible
│  │  │  ├─ classify.rs          # catégorie de chaque nœud (règles + heuristiques)
│  │  │  └─ progress.rs          # événements throttlés (10 Hz)
│  │  ├─ index/                  # recherche floue (nucleo-matcher) sur l'arène
│  │  ├─ rules/                  # schéma, chargement, expansion (%LOCALAPPDATA%, globs), matching
│  │  ├─ reco/                   # recommandations + KPI à partir du scan et des règles
│  │  ├─ safety/                 # SEUL module qui supprime, déplace ou lance une commande (§ 6)
│  │  ├─ cleanup/                # dormant.rs, duplicates.rs, largest.rs, sinks.rs
│  │  ├─ timeline/               # snapshot.rs, series.rs, projection.rs, scheduler.rs
│  │  ├─ dev/                    # projects.rs (marqueurs), git.rs (date du dernier commit), caches.rs
│  │  ├─ games/                  # steam.rs (vdf/acf), epic.rs, gog.rs, battlenet.rs, ea.rs, ubisoft.rs, shaders.rs
│  │  ├─ journal/                # écriture SQLite + export texte/CSV dans %LOCALAPPDATA%\Lumen\logs
│  │  ├─ report/                 # rapport HTML autonome + CSV
│  │  ├─ win/                    # #[cfg(windows)] : known folders, IFileOperation, Restart Manager,
│  │  │                          # miniatures (IShellItemImageFactory), menu Explorateur, tray, toasts
│  │  └─ elevated/               # point d'entrée de l'assistant élevé (§ 6.4)
│  └─ tests/
│     ├─ safety_*.rs             # denylist, canonisation, jonctions, TOCTOU, verrous (Windows en CI)
│     ├─ fixtures/mod.rs         # générateur d'arborescence de test
│     └─ rules_*.rs, duplicates.rs, smart_move.rs
├─ scripts/                      # check-contrast.mjs, check-tokens.mjs, a11y, captures
└─ .github/workflows/windows.yml # safety d'abord (bloquant), check, clippy, tests, audits, build, signature
```

Règles de dépendance : `core/` (TS) ne dépend de rien ; `features/` → `core/`, `ipc/`, `components/` ; seul `ipc/client.ts` parle à Tauri.
Côté Rust : seul `safety/` contient des appels qui modifient le disque ou lancent un processus. C'est **vérifié par l'outillage** :
`clippy.toml` interdit `std::fs::remove_*`, `std::fs::rename`, `std::process::Command::new`, `trash::*` partout
(`disallowed_methods`), et seul `safety/` porte `#[allow(clippy::disallowed_methods)]`. Un test CI vérifie que cet `allow` n'apparaît nulle part ailleurs.

---

## 3. Commandes IPC Tauri

Toutes typées (structs serde + `specta`), bindings TS générés par `tauri-specta` : le front ne peut pas appeler une commande avec un mauvais type.
**Aucun chemin libre n'entre** : le front envoie des `NodeRef { scan: ScanId, node: u32 }`, des `RuleId`, des `RecoId`, des `SelectionId`.
Les seuls chemins qui viennent de l'utilisateur passent par des dialogues natifs ouverts **côté Rust** (dossiers exclus, export, cible d'un déplacement).

| Domaine | Commande | Retour |
|---|---|---|
| App | `app_info()` | version, élevé ?, LastAccess fiable ? |
| | `settings_get()`, `settings_update(patch: SettingsPatch)` | `Settings` |
| | `settings_pick_excluded_folder()` / `settings_remove_excluded(id)` | dialogue natif côté Rust |
| Volumes | `volumes_list()` | `Vec<Volume>` (lettre, nom, FS, total, libre, fixe/amovible/réseau, `mft_capable`) |
| Scan | `scan_start(target: ScanTarget, method: Auto\|Walk\|Mft)` | `ScanId` ; `ScanTarget = UserProfile \| Volume(VolumeId) \| Node(NodeRef)` |
| | `scan_cancel(ScanId)` | — |
| | événements `scan:progress` (octets, fichiers, dossier courant, débit), `scan:partial` (agrégats niveau 1), `scan:done`, `scan:error` | |
| Arbre | `tree_slice(node: NodeRef, depth: 1..=2, min_fraction: f32)` | `TreeSlice` : 2 niveaux, petits éléments regroupés en « Autres (n) » |
| | `node_info(NodeRef)` | chemin affiché, taille, fichiers, dernière ouverture, statut, catégorie |
| | `breadcrumb(NodeRef)` | segments cliquables |
| | `node_open(NodeRef, how: Open\|Reveal)` | les exécutables sont toujours « révélés », jamais lancés |
| Reco/KPI | `recommendations(ScanId)` | `Vec<Recommendation>` (id, règle, statut, nom, chemin affiché, taille, nœuds concernés) |
| | `recommendation_detail(RecoId)` | phrase, fichiers concernés + proportions, aperçu du script |
| | `kpis(ScanId)` | libre après nettoyage, sans risque, à vérifier, non analysé, tendance 7 j |
| Plans | `plan_create(targets: Vec<PlanTarget>, mode: Trash\|Permanent)` | `PlanPreview` : `PlanId`, empreinte, total, éléments validés, éléments refusés + raison |
| | `plan_script(PlanId, what_if: bool)` | script PowerShell (chemins validés par `safety/` uniquement) |
| | `plan_simulate(PlanId)` | ce qui serait fait, sans rien toucher |
| | `plan_execute(PlanId, confirm: Confirmation)` | `ActionId` ; `Confirmation` = empreinte du plan affiché (+ 2e accusé si définitif) |
| | événements `action:progress`, `action:done` | |
| Nettoyage avancé | `dormant_find(ScanId, months: 3\|6\|12\|24)` | `SelectionId` + groupes par dossier + horodatage utilisé (LastAccess / LastWrite) |
| | `duplicates_start(ScanId)` → `JobId` ; événements `dup:progress` ; `duplicates_result(JobId)` | groupes, exemplaire à garder présélectionné, `SelectionId` |
| | `largest_files(ScanId, kind: Option<FileKind>)` | 100 plus gros |
| | `system_sinks()` | WinSxS, hiberfil, points de restauration, Windows.old, pagefile (tailles + actions possibles) |
| | `restore_point_create()` | passe par l'assistant élevé |
| Timeline | `timeline_series(VolumeId, range: D7\|D30\|M3\|All)` | points (date, utilisé, total) |
| | `timeline_compare(SnapshotId, SnapshotId, limit)` | dossiers qui ont le plus grossi/rétréci |
| | `timeline_projection(VolumeId)` | `Option<semaines>` (≥ 5 points, régression 30 j) |
| | `snapshot_now(VolumeId)` | — |
| Dev | `dev_projects(ScanId)` | nom, langage, dernier commit, taille source, taille artefacts (+ détail) |
| | `dev_global_caches()` | npm, pnpm, yarn, pip, uv, cargo, NuGet, Gradle, Maven, Docker |
| Jeux | `games_list()` | launcher, nom, taille, disque, dernière partie, jaquette (`lumen-thumb://`) |
| | `game_launcher_open(GameId, intent: Page\|Uninstall\|Move)` | ouvre une URI de launcher construite côté Rust (schémas autorisés) |
| Déplacement | `move_preflight(NodeRef, target: VolumeId)` | espace, verrous, denylist, estimation → `MovePlanId` |
| | `move_execute(MovePlanId, confirm)`, `move_undo(MoveId)` ; événements `move:progress` | |
| Recherche | `search(query: String, limit: u16)` | `Vec<SearchHit>` (NodeRef, nom, chemin affiché, taille, date) < 50 ms |
| Journal | `journal_list(filter)`, `journal_restore(ActionItemId)`, `journal_export_csv()` | export via dialogue natif côté Rust |
| Rapport | `report_export(format: Html\|Csv)` | dialogue natif côté Rust |
| Intégration | `explorer_menu_set(bool)`, `tray_set(bool)`, `low_space_alert_set(Option<u8>)` | |
| Tutoriel | `onboarding_done()`, `onboarding_reset()`, `tip_dismiss(TipId)` | |
| Logs | `logs_open_folder()` | |

Les contrôles de fenêtre passent par l'API fenêtre de Tauri (capability limitée à `minimize`, `toggle-maximize`, `close`, `start-dragging`).
Le presse-papiers (« Copier le script », « Copier le chemin ») utilise `navigator.clipboard` : le texte vient toujours du backend.

---

## 4. Modèles de données

### 4.1 Scan (en mémoire, Rust)

Une arène compacte par scan, pensée pour plusieurs millions d'entrées (~40 octets par nœud + noms) :

```rust
pub struct Scan {
    pub id: ScanId,
    pub root: RootInfo,               // volume (n° de série, FS), chemin racine canonique, méthode Walk|Mft
    pub nodes: Vec<Node>,             // index = NodeId (u32) ; 0 = racine
    pub names: NameBuffer,            // noms UTF-16 → UTF-8 concaténés, (offset u32, len u16) dans Node
    pub children: Vec<u32>,           // listes d'enfants contiguës, triées par taille décroissante
    pub last_access_reliable: bool,   // NtfsDisableLastAccessUpdate lu dans le registre
    pub errors: ErrorCounts,          // accès refusé, chemins trop longs, verrous
}

pub struct Node {
    pub parent: u32,
    pub name: NameRef,
    pub first_child: u32, pub child_count: u32,
    pub size: u64,            // taille logique (cumulée pour un dossier)
    pub alloc: u64,           // taille sur disque (cumulée) — c'est elle qui libère de l'espace
    pub files: u32,           // nombre de fichiers (cumulé)
    pub mtime: i64, pub atime: i64, pub ctime: i64,   // secondes Unix ; atime cumulé = max des enfants
    pub file_id: u64,         // FRN NTFS (128 bits sur ReFS : table annexe)
    pub attrs: u16,           // DIRECTORY, REPARSE, SYSTEM, HIDDEN, HARDLINK(n>1), ...
    pub category: Category,   // u8
    pub rule: Option<RuleIdx>,// u16 : règle qui revendique ce nœud
    pub status: Status,       // Safe | Review | Keep | Unanalyzed
}
```

- Le front ne reçoit jamais l'arène : `tree_slice` en envoie 2 niveaux, déjà regroupés.
- Les liens physiques (même `file_id`) ne sont comptés qu'une fois et ne sont jamais des doublons.
- Les reparse points ne sont jamais traversés (taille 0, statut « à garder », marqués dans l'infobulle).
- Persistance : le dernier scan de chaque racine est écrit dans SQLite (`scan_nodes`) en tâche de fond, pour que la recherche et les écrans marchent dès le lancement suivant.

### 4.2 Règles (`src-tauri/rules/rules.toml`)

```toml
[[rule]]
id          = "npm-cache"
category    = "package-caches"       # documents-media | code-projects | package-caches | dev-tools
                                     # android-sdk | ai-models | games | system
status      = "safe"                 # safe | review
name        = "Cache npm"
explanation = "Le cache npm ne contient que des copies de paquets déjà publiés : npm les retélécharge à la demande."
paths       = ["%LOCALAPPDATA%\\npm-cache"]
globs       = []                     # ex. "%USERPROFILE%\\**\\node_modules" (insensible à la casse)
scope       = "contents"             # whole | contents | files-older-than
min_age_hours = 0                    # %TEMP% : 48
command     = { program = "npm", args = ["cache", "clean", "--force"] }   # optionnelle
verify      = { program = "npm", args = ["cache", "verify"] }             # optionnelle
prerequisites = []                   # ex. [{ program = "gradle", args = ["--stop"] }]
requires_elevation = false
project_artifacts = []               # pour les artefacts : ["node_modules", ".next", "dist", "build"]
project_inactive_days = 0            # node_modules des projets sans commit depuis 6 mois : 180
```

- `program` n'est pas un chemin : c'est un nom pris dans la **liste fermée** du code Rust (`npm`, `pnpm`, `yarn`, `pip`, `uv`, `gradle`, `dotnet`, `docker`, `ollama`, `wsl`, `dism`, `powercfg`, `vssadmin`, `cleanmgr`, `diskpart`, `powershell` pour `Optimize-VHD` uniquement). Le binaire est résolu en chemin absolu côté Rust ; une règle qui cite un programme inconnu fait échouer le chargement (et un test).
- Les arguments sont des tableaux, jamais concaténés. Les seuls arguments variables (nom de modèle Ollama, chemin de vhdx) sont typés et validés (regex stricte / chemin validé par `safety/`).
- Règles prévues au jalon 4 : celles du brief (npm, artefacts des projets actifs, Gradle avec `gradle --stop`, `%TEMP%` > 48 h, pip/uv, D3DSCache + NVIDIA DXCache/GLCache + AMD ; Android SDK/AVD, Ollama `ollama rm`, vhdx Docker/WSL `wsl --shutdown` puis `Optimize-VHD` ou `diskpart compact vdisk` sur Windows Famille, node_modules inactifs, Windows.old via cleanmgr, dormants, doublons, jeux).
- Un test charge `rules.toml`, vérifie le schéma, l'unicité des ids, qu'aucun chemin de règle ne tombe dans la denylist et que les règles « review » ne sont jamais présélectionnées.

### 4.3 Snapshots

Léger : tailles agrégées par dossier sur **4 niveaux max**, jamais la liste des fichiers. Dossiers sous 50 Mo regroupés dans leur parent pour borner la taille (quelques milliers de lignes par snapshot).

### 4.4 Schéma SQLite (`%LOCALAPPDATA%\Lumen\lumen.db`, WAL)

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);           -- schema_version, onboarding, astuces vues
CREATE TABLE settings (key TEXT PRIMARY KEY, json TEXT NOT NULL);

-- Dernier scan par racine (index de recherche persistant)
CREATE TABLE scans (
  id INTEGER PRIMARY KEY, root TEXT NOT NULL, volume_serial INTEGER NOT NULL, fs TEXT NOT NULL,
  method TEXT NOT NULL CHECK (method IN ('walk','mft')),
  started_at INTEGER NOT NULL, finished_at INTEGER, status TEXT NOT NULL,
  total_alloc INTEGER, files INTEGER, dirs INTEGER, errors INTEGER, last_access_reliable INTEGER NOT NULL
);
CREATE TABLE scan_nodes (
  scan_id INTEGER NOT NULL REFERENCES scans(id) ON DELETE CASCADE,
  id INTEGER NOT NULL, parent INTEGER NOT NULL, name TEXT NOT NULL,
  size INTEGER NOT NULL, alloc INTEGER NOT NULL, files INTEGER NOT NULL,
  mtime INTEGER, atime INTEGER, ctime INTEGER, file_id INTEGER, attrs INTEGER NOT NULL,
  category INTEGER NOT NULL, rule_id TEXT, status INTEGER NOT NULL,
  PRIMARY KEY (scan_id, id)
) WITHOUT ROWID;

-- Timeline
CREATE TABLE snapshots (
  id INTEGER PRIMARY KEY, volume_serial INTEGER NOT NULL, root TEXT NOT NULL, taken_at INTEGER NOT NULL,
  used INTEGER NOT NULL, total INTEGER NOT NULL, free INTEGER NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('scan','daily'))
);
CREATE INDEX snapshots_by_volume ON snapshots(volume_serial, taken_at);
CREATE TABLE snapshot_paths (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, depth INTEGER NOT NULL);
CREATE TABLE snapshot_sizes (
  snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  path_id INTEGER NOT NULL REFERENCES snapshot_paths(id),
  alloc INTEGER NOT NULL, files INTEGER NOT NULL,
  PRIMARY KEY (snapshot_id, path_id)
) WITHOUT ROWID;

-- Journal
CREATE TABLE actions (
  id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL, finished_at INTEGER,
  kind TEXT NOT NULL,          -- clean | dormant | duplicates | dev-purge | system | shader-cache | move | move-undo | restore
  method TEXT NOT NULL,        -- trash | permanent | official-command | move
  rule_id TEXT, plan_hash TEXT NOT NULL, simulated INTEGER NOT NULL,
  status TEXT NOT NULL,        -- ok | partial | failed | cancelled
  bytes_freed INTEGER NOT NULL DEFAULT 0,
  items_ok INTEGER NOT NULL DEFAULT 0, items_locked INTEGER NOT NULL DEFAULT 0, items_refused INTEGER NOT NULL DEFAULT 0,
  script TEXT, command TEXT, error TEXT
);
CREATE INDEX actions_by_date ON actions(started_at);
CREATE TABLE action_items (
  id INTEGER PRIMARY KEY, action_id INTEGER NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
  path TEXT NOT NULL, size INTEGER NOT NULL,
  result TEXT NOT NULL,        -- done | locked | refused | missing | changed
  reason TEXT,
  recycle_item TEXT,           -- nom d'analyse de l'élément dans la Corbeille (restauration)
  restored_at INTEGER
);
CREATE TABLE moves (
  id INTEGER PRIMARY KEY, action_id INTEGER NOT NULL REFERENCES actions(id),
  source TEXT NOT NULL, target TEXT NOT NULL,
  state TEXT NOT NULL,         -- planned | copying | verifying | swapping | junction | cleaning | done | rolled-back | undone
  bytes INTEGER NOT NULL, files INTEGER NOT NULL, hashed INTEGER NOT NULL, updated_at INTEGER NOT NULL
);

-- Cache des empreintes (doublons) : évite de rehacher ce qui n'a pas changé
CREATE TABLE hash_cache (
  volume_serial INTEGER NOT NULL, file_id INTEGER NOT NULL, size INTEGER NOT NULL, mtime INTEGER NOT NULL,
  head_hash BLOB, full_hash BLOB, PRIMARY KEY (volume_serial, file_id)
) WITHOUT ROWID;
```

Le journal est aussi écrit en texte (une ligne par action) dans `%LOCALAPPDATA%\Lumen\logs\lumen-AAAA-MM.log`, rotation mensuelle, 12 fichiers max.

---

## 5. Front

- **Treemap** : squarified maison (`core/treemap/squarify.ts`, ~80 lignes, testée) sur 2 niveaux, rendue en `div` absolus (pas de canvas) pour garder texte net, focus clavier, hachures CSS, transitions d'opacité. Les petites tuiles (< seuil en px) sont regroupées en « Autres ». Zoom au clic = `tree_slice` du nœud + fil d'Ariane ; transition de 150 ms, coupée par `prefers-reduced-motion`.
- **Coloration** : Catégorie (7 pastels), Nettoyage (statuts + hachures), Âge (dégradé neutre de 5 paliers du plus récent au plus ancien, sur la dernière ouverture ; légende adaptée).
- **Tokens** : `src/styles/tokens.css` déclare tout sur `:root` (clair) et `[data-theme="dark"]` ; le mode « Système » suit `prefers-color-scheme` (réglage Windows) via `theme-boot.js` + écouteur. Transition de 150 ms sur `background-color, color, border-color`, désactivée sous `prefers-reduced-motion`. Stylelint interdit tout hex/rgb hors `tokens.css`.
- **Transposition sombre** des neutres de la maquette (proposition, à vérifier au contraste) : `--text-2 #D6D3D1`, `--text-3 #A8A29E` (labels du brief), `--surface-sunken #1F1E1C`, `--border-soft #22211F`, delta vert `#6FCF9E`, delta ambre `#F0B45A`, `--action-clean #2F9E6E` avec texte `#0C0C0B`.
- **Contraste** : test qui calcule le ratio de chaque paire texte/fond (noms et tailles de tuiles sur les 7 catégories + 3 statuts, avec et sans hachures, × 2 thèmes). Constat dès maintenant : `#78716C` sur `#DCD6F7` ≈ 3,4:1 (échoue) ; c'est pour ça que la maquette utilise `#44403C` / `#57534E` sur les tuiles, et ce sont ces jetons que les tuiles utiliseront.
- **Polices** : `@fontsource-variable/inter` et `@fontsource/jetbrains-mono` (woff2 locaux, sous-ensembles latin + latin-ext). CSP `font-src 'self'`.
- **Tests** : vitest + Testing Library (jsdom) ; Playwright pour les captures de comparaison avec la maquette et axe-core.

---

## 6. Module `safety/` (cœur non négociable)

### 6.1 API

```rust
// Seules entrées publiques. Tout le reste du crate passe par là.
pub fn plan(targets: &[PlanTarget], mode: DeleteMode, ctx: &SafetyCtx) -> Result<Plan, SafetyError>;
pub fn script(plan: &Plan, what_if: bool) -> String;
pub fn simulate(plan: &Plan, ctx: &SafetyCtx) -> SimReport;
pub fn execute(plan: &Plan, confirmation: &Confirmation, ctx: &SafetyCtx, progress: impl Progress) -> Outcome;
pub fn smart_move(plan: &MovePlan, ...) -> MoveOutcome;  pub fn undo_move(id: MoveId, ...) -> MoveOutcome;
```

`Plan` n'est constructible que par `safety::plan` (champs privés) et porte une empreinte BLAKE3 de son contenu : `execute` refuse une `Confirmation` dont l'empreinte diffère de celle affichée.

### 6.2 Vérifications, dans l'ordre, pour chaque élément

1. **Canonisation** : ouverture avec `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`, puis `GetFinalPathNameByHandleW` (résout `PROGRA~1`, la casse, `..`) ; normalisation `\\?\` / `\\?\UNC\` ; refus des chemins relatifs, des chemins de périphérique `\\.\`, des flux `:$DATA`, des noms réservés (`CON`, `NUL`…).
2. **Liste blanche** : le chemin canonique est couvert par une règle (chemins/globs expansés, `scope`, âge minimum) **ou** appartient à une `SelectionId` produite par Dormants/Doublons et cochée par l'utilisateur.
3. **Denylist** (codée en dur) : comparaison par composants, insensible à la casse (pliage Unicode simple de NTFS), sur la lettre réelle de `%SystemDrive%` *et* sur `C:` ; known folders résolus (`FOLDERID_Windows`, `ProgramFiles`, `ProgramFilesX86`, `ProgramData`, `Public`, `Profiles`). Contenu : liste du brief, plus racines de volumes, chemins `\\?\Volume{…}` (EFI), `$*` à n'importe quel niveau, ruches, profils `Default`/`Public`/autres utilisateurs (via `ProfileList` du registre), dossier `.git` entier ou dossier contenant `.git` (seuls les artefacts listés y sont autorisés).
4. **Reparse points** : l'élément et **chaque parent** jusqu'à la racine du volume sont vérifiés en direct (pas seulement au scan).
5. **Anti-TOCTOU** : même `file_id` + n° de série du volume, même taille, même date de modification qu'au scan ; sinon refus (`changed`). Le handle reste ouvert (partage lecture/écriture/suppression) jusqu'à l'action pour réduire la fenêtre.
6. **Attribut SYSTEM** → refus ; **verrou** (ouverture exclusive impossible / Restart Manager) → ignoré et compté (`locked`), jamais d'erreur bloquante.

Pour une **commande officielle**, on valide la règle (programme dans la liste fermée, prérequis, élévation) et les éventuels arguments variables ; pour un gouffre système, uniquement les commandes du brief (DISM, powercfg, `vssadmin delete shadows /for=C: /oldest`, cleanmgr), jamais de chemin.

### 6.3 Exécution

- Corbeille par défaut via `IFileOperation` (`FOFX_RECYCLEONDELETE`, `FOF_NOCONFIRMATION`, sans UI) ; le `IFileOperationProgressSink` récupère l'élément créé dans la Corbeille → `action_items.recycle_item` pour « Restaurer ».
- Suppression définitive : option désactivée par défaut, double confirmation affichant taille et nombre.
- Commande officielle préférée quand elle existe (le panneau de détail le dit, comme la maquette : « ces fichiers ne passent pas par la Corbeille »).
- Simulation par défaut partout ; une vraie exécution exige une confirmation qui montre le total et la liste.
- Scripts : générés uniquement depuis un `Plan` validé, chemins absolus entre apostrophes (apostrophes doublées, caractères de contrôle refusés), `-WhatIf` par défaut via une fonction `SupportsShouldProcess` qui envoie à la Corbeille (`Microsoft.VisualBasic.FileIO`), commentaires en français.

### 6.4 Élévation

L'appli tourne sans droits admin. Pour le scan MFT et les actions système, Lumen lance **un assistant élevé** (`lumen.exe --elevated`, `ShellExecuteEx` + `runas`) après une phrase d'explication dans l'UI. Il reçoit sur un tube nommé (ACL : utilisateur courant uniquement, PID parent vérifié) une **tâche typée** parmi une liste fermée (`MftScan { volume }`, `SystemAction { rule_id }`, `RestorePoint`), refait lui-même les vérifications de `safety/`, puis se termine. L'interface ne tourne jamais élevée.

### 6.5 Déplacement intelligent

Machine à états persistée dans `moves` (reprise/rollback après un plantage) :
`preflight` (denylist source et cible, volume local NTFS — une jonction ne peut pas viser un lecteur réseau —, espace + 5 %, aucun fichier verrouillé) → `copying` (vers `cible\nom.lumen-partial`) → `verifying` (nombre de fichiers, tailles, BLAKE3 obligatoire) → `swapping` (renommage atomique de l'original en `nom.lumen-old` sur le même volume) → `junction` (création de la jonction, puis lecture d'un fichier à travers elle) → `cleaning` (suppression de `nom.lumen-old`) → `done`.
Toute erreur avant `cleaning` ⇒ rollback automatique (jonction retirée, original renommé à sa place, copie partielle supprimée). « Annuler le déplacement » rejoue le chemin inverse avec les mêmes vérifications.
Décision : l'original est supprimé **définitivement** (la Corbeille du même disque ne libérerait rien), avec ces garde-fous :
- vérification BLAKE3 **obligatoire** (pas en option) avant toute suppression ;
- la jonction est créée **et testée** (lecture d'un fichier de la cible à travers elle) avant la suppression ;
- confirmation explicite : « l'original sera supprimé définitivement, pas mis à la Corbeille » ;
- rollback possible à toutes les étapes avant la suppression ;
- « Annuler le déplacement » recopie depuis la cible et retire la jonction.

### 6.6 Tests (bloquants)

- Logique pure (canonisation lexicale, denylist, liste blanche, génération de script) testée **sur Linux et Windows** grâce à un trait `FsProbe` (implémentation Windows réelle + implémentation factice).
- Tests Windows en CI sur une arborescence générée (`tests/fixtures`) : jonction vers `C:\Windows`, symlink de fichier, parent reparse, fichier verrouillé, chemin > 260 caractères, noms Unicode, `PROGRA~1`, `\\?\C:\WINDOWS\..`, casse mixte, chemins relatifs, attribut SYSTEM, fichier modifié entre scan et action (TOCTOU), ruches, `$Recycle.Bin`, profils.
- Le job CI exécute `cargo test safety` **en premier** ; s'il échoue, rien d'autre ne tourne et aucun installeur n'est produit.

---

## 7. Scanner

- **Walk** (défaut, non élevé) : `jwalk` parallèle (rayon), `FindFirstFileExW` avec `FIND_FIRST_EX_LARGE_FETCH` pour les métadonnées, taille sur disque via `GetCompressedFileSizeW` seulement pour les fichiers compressés/clairsemés. Ne suit jamais les reparse points. Annulable (drapeau atomique vérifié à chaque répertoire).
- **MFT** (élevé, NTFS uniquement) : lecture brute de `$MFT` via `\\.\C:` dans l'assistant élevé, analyse des enregistrements FILE (`$STANDARD_INFORMATION`, `$FILE_NAME`, `$DATA` résident/non résident, `$ATTRIBUTE_LIST`), reconstruction de l'arbre par FRN parent ; arène transmise au processus normal. Repli automatique sur Walk en cas d'erreur.
- **Progression** : événements à 10 Hz max, agrégats partiels du niveau 1 pour que la treemap se remplisse pendant le scan.
- **Dernière ouverture** : `LastAccess` si `NtfsDisableLastAccessUpdate` indique qu'il est maintenu, sinon `LastWrite` ; l'UI l'indique (« d'après la dernière modification »).
- **Recherche** : `nucleo-matcher` en parallèle sur les noms de l'arène, résultats triés score puis taille ; objectif < 50 ms pour 2 M d'entrées (banc dans les tests).

---

## 8. Sécurité de l'appli Tauri

- Capabilities minimales, aucune permission `shell`, `fs`, `http` côté front. `opener` non exposé au front : l'ouverture passe par `node_open` / `game_launcher_open` (schémas `steam://`, `com.epicgames.launcher://`, `goggalaxy://`, `battlenet://`, `origin2://`, `uplay://` construits côté Rust avec un id validé).
- CSP stricte : `default-src 'self'; img-src 'self' lumen-thumb: data:; font-src 'self'; style-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost`. Miniatures servies par le protocole `lumen-thumb://<NodeRef>` (jamais un chemin).
- Aucun accès réseau : pas de crate HTTP dans `Cargo.toml` (vérifié par `cargo deny` / test sur l'arbre de dépendances).
- `cargo audit`, `cargo deny`, `npm audit --audit-level=high` dans la CI ; signature Authenticode via `bundle.windows.signCommand` avec un placeholder (`LUMEN_SIGN_CERT`, étape sautée si absent).
- Aucun `unwrap()`/`expect()` hors tests : `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` en `deny`. Erreurs `thiserror`, chaque variante a son message français.

---

## 9. Jalons (arrêt après chacun pour que tu testes)

| # | Contenu | Testable par toi |
|---|---|---|
| 1 | Projet Tauri, UI statique fidèle à la maquette (chiffres de la maquette), navigation segmentée, thèmes clair/sombre/système, bascule titre, Paramètres et palette Ctrl K en squelette, `CLAUDE.md`, CI de base | `npm run tauri dev`, comparaison avec la maquette (captures côte à côte) |
| 2 | `safety/` complet + tests (Linux + Windows CI), clippy anti-suppression hors `safety/` | `cargo test safety`, CI verte |
| 3 | Scanner jwalk puis MFT (assistant élevé), progression, sélecteur de disque, treemap réelle, zoom, infobulle, menu contextuel (actions non destructives) | scanner ton profil et C: |
| 4 | `rules.toml`, recommandations, KPI calculés, survol qui estompe la treemap, panneau de détail | vérifier les reco sur ta machine |
| 5 | Scripts, copie, simulation, confirmation, exécution (Corbeille / commande officielle), journal | nettoyer un cache npm réel |
| 6 | Dormants, doublons (BLAKE3 + miniatures), gros fichiers, gouffres système + point de restauration | |
| 7 | Snapshots, planificateur quotidien, Timeline, comparaison, projection, mention KPI | |
| 8 | Dev (projets, artefacts, caches globaux, Docker) et Jeux (6 launchers, shaders) | |
| 9 | Déplacement intelligent, recherche instantanée, export HTML/CSV | |
| 10 | Onboarding 4 écrans + astuces contextuelles | |
| 11 | Menu de l'Explorateur, zone de notification, alertes, installeur NSIS signé (placeholder) | installer/désinstaller |

Chaque jalon : 0 erreur TS, 0 warning lint/clippy, tests verts, `PROGRESS.md` à jour, captures comparées à la maquette, checklist de test manuel, puis arrêt.

---

## 10. Décisions (20/10/2026)

1. **Emplacement** : dépôt dédié `shingao/lumen`.
2. **Boutons** : « Nettoyer » en vert (`#15803D`, cohérent avec le statut sans risque) dans les deux thèmes ; « Copier le script » en noir, inversé en sombre (fond `#EDEBE8`, texte `#0C0C0B`).
3. **Déplacement intelligent** : suppression définitive de l'original avec les garde-fous du § 6.5.
4. **Caches sans commande officielle** : registre cargo et `.m2` « À vérifier » ; `pnpm store prune` (ne retire que le non référencé), `yarn cache clean`, `dotnet nuget locals all --clear` « Sans risque ».
5. **Langue et unités** : interface en français ou en anglais selon la langue de Windows (réglable) ; tailles en base 1024 comme l'Explorateur ; « Go » avec virgule décimale en français (Ko, Mo, Go, To), « GB » avec point en anglais (KB, MB, GB, TB).
