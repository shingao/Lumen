# Lumen — guide du projet

Appli desktop Windows 11 qui explique pourquoi le disque est plein et aide à le libérer **sans risque**. Remplace SpaceSniffer : plus épuré, plus beau, plus utile.

## Sources de vérité

| Sujet | Fichier |
|---|---|
| Visuel (espacements, tailles, couleurs, rayons, hachures) | `design/lumen-mockup.html` (+ captures `design/maquettes/`) |
| Architecture (IPC, modèles, schéma SQLite, sécurité) | `docs/PLAN.md` |
| Avancement, décisions, écarts avec la maquette | `PROGRESS.md` |
| Tokens CSS | `src/styles/tokens.css` |

Tout nouvel écran prolonge exactement la DA de la maquette : mêmes conteneurs, même densité, chiffres et chemins en mono, pastels, hachures pour le « sans risque ». Aucun composant qui détonne.

## Méthode

- Avancer **jalon par jalon** (liste dans `PROGRESS.md`), s'arrêter après chacun pour que l'utilisateur teste. Ne pas démarrer le suivant sans son accord.
- Fin de jalon : `npm run check` vert, `cargo clippy --workspace --all-targets -- -D warnings` et `cargo test --workspace` verts, captures comparées à la maquette (`npm run captures`), `PROGRESS.md` à jour (fait / reste / décisions / écarts), checklist de test manuel, commit, push, **PR vers `main`**. Un jalon n'est terminé qu'avec la CI Windows verte.
- **Une branche et une PR par jalon** (depuis le jalon 4 : `claude/jalon-4-regles`…), PR vers `main`. Commits en anglais, `milestone N: …`.

## Stack

- Tauri 2 + Rust ; React 19 + TypeScript strict + Vite ; **aucune** librairie de composants ni d'icônes (SVG maison dans `src/components/icons.tsx`).
- CSS fait main : CSS modules (`*.module.css`) + variables de `tokens.css`. Stylelint interdit hex/rgb/hsl, `ms`/`s`, tailles et graisses en dur hors `src/styles/tokens*.css`.
- Polices embarquées : Inter (variable) et JetBrains Mono via `@fontsource` (aucun CDN).
- Treemap squarified maison : `src/core/treemap/squarify.ts` (2 niveaux, gap 3 px).
- Analyse : crate `src-tauri/scan/` — parcours parallèle maison (rayon) par handle de dossier (`FileIdExtdDirectoryInfo`, puis chaque fichier ouvert par file ID pour ses attributs seulement), MFT pour NTFS en administrateur (assistant élevé `--lumen-mft-scan`).
- Journal des nettoyages : SQLite (rusqlite `bundled`) dans `%LOCALAPPDATA%\Lumen\lumen.db` (`src-tauri/src/journal.rs`, schéma versionné) ; nettoyage côté appli : `src-tauri/src/cleanup.rs` (recommandations → cibles, autorisations et commandes pour `safety`, IPC du plan et du journal) ; nettoyage avancé : `src-tauri/src/advanced/` (dormants, gros fichiers, doublons avec cache des empreintes dans le journal, gouffres système, IPC `advanced_plan`), mêmes plan, exécution et journal.
- Timeline : `src-tauri/src/timeline/` (`snapshot.rs` tailles agrégées par dossier sur 4 niveaux, jamais de fichiers ; `store.rs` SQL ; `retention.rs` ; `projection.rs` ; `diff.rs` ; `mod.rs` IPC `timeline_*` et snapshot quotidien), dans la même base que le journal (migrations `MIGRATIONS` de `journal.rs`, chacune testée depuis une base vide et depuis la version précédente). **Un volume est identifié par son GUID** (à défaut numéro de série + système de fichiers), **jamais par sa lettre** ; racines et chemins des snapshots relatifs au volume. Une série = volume + racine + méthode : on ne compare jamais deux séries (refus expliqué, pas de faux delta). Snapshot à chaque analyse et un par jour seulement tant que l'appli tourne (`dailyTick` côté front, jamais d'UAC, ni service ni tâche planifiée). Front : `src/app/timeline.ts`, `src/features/timeline/`, `src/core/timeline.ts` ; ouverture d'un dossier dans l'Explorer depuis un autre écran : `openPath` de `src/app/navigation.ts`.
- Analyses de la session : `scanning.rs` garde la dernière analyse de chaque cible (4 au plus) ; tout retour à un disque déjà analysé passe par `showCachedScan` (`src/app/scanning.ts`) avant de relancer un scan.
- Jeux : crate `src-tauri/launchers/` (détection **en lecture seule** : Steam `libraryfolders.vdf` / `appmanifest_*.acf` / `localconfig.vdf`, Epic, GOG, Battle.net, EA, Ubisoft, repli Uninstall ; parseur VDF tolérant ; trait `Registry` avec `MemRegistry` pour les tests) ; `src-tauri/src/games/` (IPC `games_*`, jaquettes du cache local seulement, taille mesurée à défaut, caches de shaders) ; front `src/app/devgames.ts`, `src/features/games/`. Toute lecture de fichier de launcher ou de projet est **tolérante** : absent, illisible, verrouillé, trop gros (8 Mio) ou mal formé = ignoré et compté (`Ignored`), jamais une erreur qui arrête tout. Dernière partie inconnue = « inconnue », jamais une date inventée.
- Dev : `src-tauri/src/dev/` (projets trouvés dans l'analyse affichée, artefacts = nœuds couverts par `artefacts-projets-*` seulement, `select_artifacts` refuse tout autre nœud ; caches globaux = recommandations « packages » ; `docker.rs` ; `wsl.rs` compactage), front `src/features/dev/`.

## Tokens (résumé — détail dans `src/styles/tokens.css`)

| Rôle | Clair (`:root`) | Sombre (`[data-theme="dark"]`) |
|---|---|---|
| Fond appli / conteneurs / bordures | `#FAFAF9` / `#FFFFFF` / `#E7E5E4` | `#0C0C0B` / `#161615` / `#2A2826` |
| Texte / labels | `#1C1917` / `#78716C` | `#EDEBE8` / `#A8A29E` |
| Documents & médias | `#DCD6F7` | `#34304A` |
| Projets de code | `#F8D7DA` | `#452A2E` |
| Caches de paquets | `#CDEBE0` | `#1E3A31` |
| Outils dev & autres | `#FBE3CF` | `#45342A` |
| SDK Android / émulateurs | `#D6E6F5` | `#22344A` |
| Modèles IA | `#E2E8F0` | `#2A3039` |
| Jeux | `#F3E8C9` | `#3F3823` |
| Sans risque (fond / pastille) | `#D3ECDF` / `#2F9E6E` | `#173528` / `#3FBF87` |
| À vérifier | `#F7E2B4` / `#D98A1C` | `#43351A` / `#F0A33A` |
| À garder | `#EEECE9` | `#22211F` |
| « Copier le script » (primaire) | fond `#1C1917`, texte `#FFFFFF` | fond `#EDEBE8`, texte `#0C0C0B` |
| « Nettoyer » | vert `#15803D`, texte blanc | identique |
| Hachures (135°, 1 px tous les 6 px) | `rgba(28,25,23,.17)` | `rgba(237,235,232,.14)` |

- Conteneurs : rayon 16, bordure 1 px, pas d'ombre lourde. Tuiles : rayon 6 (sous-tuiles 5), espacement 3 px.
- Neutres de la maquette : `--text-2` `#292524`, `--text-3` `#44403C`, `--text-4` `#57534E`, `--bg-sunken` `#F5F5F4`, `--border-soft` `#F0EEEC` (transposés en sombre).
- Contraste AA vérifié par `src/styles/contrast.test.ts` (noms/tailles/gains sur toutes les couleurs de catégorie, de statut et d'âge, avec et sans voile, × 2 thèmes). Toute nouvelle paire texte/fond s'y ajoute.
- Thème : Système (défaut, suit Windows) / Clair / Sombre ; `src/app/theme.ts` + `public/theme-boot.js` (pas de flash) ; transition 150 ms coupée par `prefers-reduced-motion`.

## Langue et unités

- **Le français est la langue de référence** : textes écrits d'abord dans `src/i18n/fr.ts`, `en.ts` en reprend exactement les clés ; repli en français si la langue de Windows n'est ni le français ni l'anglais. Réglable (Système / Français / English). Accès via `useT()` / `currentT()`. Aucune chaîne visible en dur.
- Tailles en **base 1024** comme l'Explorateur : `13,4 Go` (fr, Ko/Mo/Go/To) ou `13.4 GB` (en). Signe moins U+2212. Tout passe par `src/core/format.ts`.

## Règles de sécurité (non négociables)

- Toute suppression, tout déplacement et toute commande passent par **un seul crate Rust `src-tauri/safety/`**.
  - Logique pure, testable partout : `path.rs` (canonisation lexicale), `denylist.rs`, `allow.rs` (liste blanche), `check.rs` (pipeline dans l'ordre), `plan.rs` (plan avec contenu attendu des dossiers et commandes, simulation, empreinte BLAKE3, confirmation, `execute` seul point d'entrée), `exec.rs` (recontrôle complet au moment de l'action, effets derrière le trait `Effects`), `script.rs` (PowerShell autonome), `commands.rs` (liste fermée, résolution des binaires par chemin complet). Tests portables avec une sonde en mémoire (`probe::memory`).
  - Appels Win32 : `safety::win` uniquement (`WinProbe`, liste de dossier par handle, dossiers connus, `WinEffects` : Corbeille `IFileOperation` avec emplacement `$R…`, définitif par handle, commandes dans un objet job ; `restore.rs` : restauration depuis la Corbeille ; `content.rs` : lecture du contenu pour les doublons, dernier accès gelé, E/S basse priorité, jamais un fichier cloud ; `elevated.rs` : commandes système élevées par `ShellExecuteEx` runas, arguments fixes ; `sinks.rs` : gouffres système et `NtfsDisableLastAccessUpdate`), seul endroit avec `unsafe` et `Command::new`. Ses tests (`win/tests.rs`) tournent **uniquement sur Windows, sur de vraies fixtures** (jonctions, symlinks, 8.3, `\\?\`, file ID, verrous, SYSTEM, chemins longs Unicode, Corbeille) : jamais de sonde simulée.
  - Chaque test de `safety/` est listé dans `src-tauri/safety/required-tests.txt` ; `node scripts/check-safety-tests.mjs` échoue si un test manque, est ignoré, échoue ou n'est pas exécuté. Ajouter tout nouveau test à cette liste.
  - Jeux (jalon 8) : **Lumen ne supprime ni ne déplace jamais un jeu** ; `safety::launch` (liste fermée : `steam://uninstall/<appid>`, `steam://open/settings`, exécutable d'un launcher connu sous Program Files) ouvre le launcher. `safety::guard::ProcessGuard` (processus des launchers et des dossiers de jeux) fait partie du plan et de son empreinte : `exec::run` n'exécute rien si l'un tourne (caches de shaders). Docker : `image prune` sans `--all` (images sans étiquette, listées par `DockerDanglingImages`), seulement si Docker répond. Compactage WSL (`OfficialCommand::VhdCompact`, réglage Expérimental `experimentalWsl`, désactivé par défaut) : `wsl --shutdown` prérequis obligatoire, script élevé lancé par `ShellExecuteEx` runas, `Optimize-VHD` en lecture seule ou diskpart `attach vdisk readonly` ; **jamais de suppression d'un vhdx**.
  - Exécution réelle (jalon 5) : `plan::execute` après l'empreinte du plan affiché (+ seconde confirmation du définitif) ; **chaque élément repasse les 6 étapes au moment de l'action, contenu des dossiers compris, élément par élément** ; tout ce qui a changé est sauté et signalé, jamais supprimé, et un dossier qui a changé n'est jamais traité d'un bloc. Définitif : identité relue sur le handle ouvert, jamais à travers un lien. Commandes : chemin complet résolu (jamais le PATH seul), prérequis d'abord, délai maximal, sorties capturées ; une commande « administrateur » n'est lancée que pour les gouffres système et le compactage WSL (plan élevé, une demande UAC par commande, arguments fixes). Le front n'envoie jamais de chemin libre : seulement des ids d'éléments du scan, des ids de règles ou de sélections. `clippy.toml` interdit `std::fs::remove_*`, `rename`, `copy`, `Command::new` ailleurs.
- Avant chaque action, dans l'ordre : canonisation (`\\?\`, 8.3, casse, `..`) → liste blanche (règle ou sélection Dormants/Doublons) → denylist → aucun reparse point sur le chemin ni ses parents → anti-TOCTOU (file ID, taille, date de modif identiques au scan) → fichier non verrouillé (sinon ignoré et compté).
- Doublons : `safety` refuse toute sélection qui retirerait tous les exemplaires d'un groupe ou deux noms d'un même fichier (liens physiques), vérifie au moment de l'action qu'un exemplaire non sélectionné est intact et recalcule l'empreinte de l'élément (contenu changé → sauté). Commandes « administrateur » (gouffres système) : seulement si le plan l'autorise (`with_elevation`), lancées directement par `ShellExecuteEx` runas, jamais par l'assistant MFT ; point de restauration = prérequis obligatoire, sauf « Continuer sans point de restauration » confirmé explicitement (`Confirmation::without_restore_point`, journalisé). Le plus ancien point de restauration n'est jamais supprimé s'il en reste moins de 2 ou s'il date de moins de 7 jours ou de la session (`commands::oldest_deletable`, revérifié par le script au moment de l'action, sur le cliché affiché).
- Corbeille par défaut (`IFileOperation`, annulable). Suppression définitive : option désactivée par défaut, double confirmation (taille + nombre). Commande officielle préférée à `Remove-Item`. Simulation par défaut ; exécution réelle après confirmation montrant total et liste. Un « À vérifier » n'est jamais coché par défaut.
- Scripts générés (`safety::script`) : chemins absolus validés par `safety/` uniquement, commentés, **simulation par défaut** (`-WhatIf` tant que `-Executer` n'est pas donné), et leurs propres garde-fous puisqu'ils s'exécutent hors de `safety` : refus au-delà de 24 h, `Test-Path`, refus si l'élément ou un parent est un lien, si la taille ou la date diffère de la génération, hors disque fixe ; Corbeille via `Microsoft.VisualBasic.FileIO`, jamais `Remove-Item -Recurse` ; `ErrorAction Stop` par élément ; journal dans un fichier. Testés par Pester en CI (`scripts/ci/script.Tests.ps1`).
- Denylist codée en dur : `C:\Windows`, `Program Files`, `Program Files (x86)`, `ProgramData` (seules les commandes officielles des gouffres système y touchent) ; racines de volumes, EFI, `\Boot`, `\EFI`, `bootmgr`, `BOOTNXT` ; `System Volume Information`, `$Recycle.Bin`, `$Extend`, tout `$*` NTFS ; `pagefile.sys`, `swapfile.sys`, `hiberfil.sys` ; ruches (`NTUSER.DAT*`, `UsrClass.dat*`, `System32\config`) ; profils `Default`, `Public` et autres utilisateurs ; attribut SYSTEM ; un dossier contenant `.git` n'est jamais supprimé en entier ; **secrets et configurations**, où qu'ils soient (même dans un cache) : `.npmrc`, `.yarnrc*`, `.git-credentials`, `.netrc`, clés `id_rsa*`/`id_ed25519*`, `*.jks`/`*.keystore`/`*.pfx`/`*.p12`/`*.kdbx`/`*.ppk`, stockage des navigateurs et applis Electron (`Cookies`, `Login Data`, `Web Data`, `Local Storage`, `Session Storage`, `IndexedDB`, `Sessions`, `logins.json`, `key4.db`…), et dans le profil `~\.ssh`, `~\.aws`, `~\.azure`, `~\.kube`, `~\.gnupg`, `~\.gradle\gradle.properties`, `~\.gradle\init.d`, `~\.m2\settings*.xml`, `~\.cargo\credentials*`, `~\.cargo\config*`, `~\.docker\config.json`, `~\.android\adbkey*` (et leurs dossiers parents).
- Déplacement intelligent : copie → vérification **BLAKE3 obligatoire** → jonction créée **et testée** (lecture d'un fichier à travers elle) → suppression **définitive** de l'original, annoncée comme telle ; rollback possible avant la suppression ; « Annuler le déplacement » recopie depuis la cible et retire la jonction.
- Onglet Gouffres système **désactivé par défaut** (Paramètres › Expérimental, `experimentalSystem`) tant qu'il n'a pas été vérifié en VM Hyper-V par l'utilisateur.
- Sans droits admin par défaut ; élévation seulement pour le scan MFT (assistant séparé) et les gouffres système / le compactage WSL (commande officielle lancée par `ShellExecuteEx` runas, jamais par l'assistant), expliquée en une phrase.
- Assistant élevé (`scan/src/win/helper.rs`) **muet et éphémère** : une seule mission (lire la MFT d'un volume désigné par sa lettre, disque fixe NTFS), aucun chemin ni commande en entrée ; tube créé par Lumen non élevé (nom aléatoire, première instance, DACL utilisateur, client en `SECURITY_IDENTIFICATION`) ; PID et exécutable du pair vérifiés des deux côtés ; jetons à usage unique ; arrêt seul après 10 s sans échange, à la fermeture de Lumen et après le scan. Exécutable vérifié avant lancement (`trust.rs` : sous Program Files, non modifiable sans droits admin) ; installeur `perMachine`. Tout nouveau processus élevé suit les mêmes règles.
- Tauri : capabilities minimales (`src-tauri/capabilities/main.json` : contrôles de fenêtre seulement), CSP stricte, aucune commande shell exposée, IPC typé et validé côté Rust. Commandes externes : liste fermée de binaires, arguments en tableau.
- **Aucun accès réseau, aucune télémétrie.**
- Aucun `unwrap()`/`expect()`/`panic!` en production (lints clippy `deny`). Erreurs `thiserror` avec message français (`src-tauri/src/error.rs`).
- Si un test de `safety/` échoue, est ignoré ou manque, le build est bloqué : le job CI `safety` (Windows) passe avant tous les autres.
- Toute nouvelle forme de suppression ou de déplacement réelle : test de bout en bout sur la CI Windows (faux profil, vérification de ce qui part et de ce qui reste, comme `src-tauri/src/e2e.rs`), et premier essai manuel dans Windows Sandbox ou une VM, jamais d'abord sur le PC principal.

## Structure

```
design/            maquette HTML (source de vérité), captures de référence, icône
docs/PLAN.md       architecture validée
src/
  app/             App, store (zustand), thème/langue, raccourcis, commandes palette, accès données
  components/      Segmented, Button, Toggle, Menu, Toast, StatusDot, icônes, style des popovers
  features/        titlebar, kpi, explorer (treemap, légende), recommendations, detail, cleanup (confirmation, progression, bilan), advanced, timeline, games, dev, journal, screens, settings, palette
  core/            logique pure testée : format, squarify, fuzzy, contrast
  ipc/             backend.ts (contrat), client.ts + tauri.ts (seuls contacts Tauri), mock/ (données de la maquette, analyse simulée), types.ts
  i18n/            fr.ts, en.ts
  styles/          tokens.css, fonts.css, global.css (+ contrast.test.ts)
src-tauri/         Rust (workspace) : lib.rs, error.rs, scanning.rs (commandes de l'analyse), analysis.rs (règles → DTO), cleanup.rs, advanced/, timeline/, games/, dev/ ; safety/ (crate garde-fou) ; scan/ (crate d'analyse) ; rules/ (moteur de règles, rules.toml, sans-risque.txt) ; launchers/ (détection des jeux, lecture seule)
scripts/           captures.mjs, make-icon.mjs, check-safety-tests.mjs ; ci/script.Tests.ps1 (Pester : script généré sur fixtures)
```

Dépendances : `core/` ne dépend de rien ; `features/` → `core/`, `ipc/`, `components/`, `app/` ; seuls `ipc/client.ts` et `ipc/tauri.ts` importent `@tauri-apps/*`.

## Analyse (règles)

- **Jamais le contenu d'un fichier** : liste des dossiers, puis chaque fichier ouvert par file ID avec `FILE_READ_ATTRIBUTES` et `FILE_FLAG_OPEN_NO_RECALL` pour lire tailles et dates dans son propre enregistrement (la copie de la liste du dossier peut être périmée : autres noms d'un lien physique…) ; `GetCompressedFileSizeW` en plus pour les fichiers compressés / clairsemés / WOF / dédupliqués. Fichiers cloud (`RECALL_ON_DATA_ACCESS`, `OFFLINE`) jamais ouverts, comptés à part ; dossiers cloud `RECALL_ON_OPEN` jamais énumérés.
- Taille **allouée** par défaut, logique en infobulle. Liens physiques comptés une fois (file ID). Points d'analyse jamais traversés. Erreurs comptées, jamais bloquantes.
- Chaque entrée garde file ID, tailles, dates (FILETIME) pour l'anti-TOCTOU de `safety`.
- **Le scan ne modifie jamais la date de dernier accès** (base des fichiers dormants) : fichiers ouverts en `FILE_READ_ATTRIBUTES` seulement, dossiers listés avec la date gelée (`SetFileTime` à `0xFFFFFFFF`), racines résolues sans lister les parents. Test `scan_sans_toucher_au_dernier_acces`.
- MFT de forme non gérée (`ScanError::MftUnsupported`) : bascule automatique sur le parcours, signalée dans l'interface.
- Parcours et MFT doivent donner les mêmes totaux : même classement (`scan/src/classify.rs`), test Windows `mft_identique_au_parcours`.
- Le front ne reçoit que des tranches agrégées (`Scan::slice_adaptive` : enfants et petits-enfants, puis jusqu'à 4 niveaux sous les dossiers d'au moins 1 % de la vue ; un regroupement « autres » garde ses membres pour son statut) ; jamais d'UAC au démarrage ; MFT seulement si déjà élevé ou après « Scan rapide (admin) ».
- Navigation de l'Explorer : `src/app/navigation.ts` (historique précédent / suivant par chemin, `scan_find`) ; tout zoom passe par `goTo`, tout changement de disque par `goToTarget`.

## Règles (`src-tauri/rules/`)

- Données dans `rules/rules.toml` (embarqué), schéma strict (`schema.rs`) : une cible commence toujours par une variable connue, jamais un chemin absolu ; textes fr (référence) + en ; commandes de la liste fermée de `safety` seulement.
- « Sans risque » = uniquement des données **régénérables** (`regenerated-by` obligatoire). `rules/sans-risque.txt` est généré par un test et relu à chaque changement (`LUMEN_UPDATE_RULE_LISTS=1 cargo test -p rules`). Un « À vérifier » n'est jamais coché par défaut.
- Le moteur ne couvre jamais un élément de la denylist (vérifié pour chaque élément et son contenu), donc jamais un secret ; une règle vise un sous-dossier de cache explicite, jamais la racine d'un dossier d'outil (test) ; rien dans Téléchargements ; règles « commande » = mesure seulement ; un élément = une seule règle (la première du fichier).
- Plus aucune catégorie devinée par nom côté front : catégories, statuts et gains viennent du moteur.
- `screen = "games"` : règle proposée seulement dans l'onglet Jeux (jalon 8) ; sur l'écran Analyse, statut et couleur dans la treemap, sans gain, ni recommandation, ni KPI (shaders de jeux).
- `.git` lu sans lancer git (`git.rs`), fichiers lus à date de dernier accès gelée (`fsread.rs`).

## Commandes

```bash
npm install
npm run dev              # front seul dans le navigateur (données factices), http://localhost:1420
npm run tauri dev        # appli Windows
npm run check            # typecheck + eslint + stylelint + vitest (dont contrastes AA)
npm run test             # vitest seul
npm run captures         # captures Playwright (serveur dev lancé) → design/captures/
npm run build            # build du front
npm run tauri build      # installeur NSIS (src-tauri/target/release/bundle/nsis/)
npm run icon             # icônes depuis design/app-icon.svg
node scripts/check-safety-tests.mjs   # tests de safety/ : présents, exécutés, aucun ignoré (tous sous Windows)
cd src-tauri && LUMEN_UPDATE_RULE_LISTS=1 cargo test -p rules   # régénère rules/sans-risque.txt après un changement de règle « Sans risque »
cd src-tauri && cargo run --release -p scan --example bench -- generate <dossier> 200000   # puis walk <dossier>, mft <dossier>
# Banc d'essai en CI : job `bench`, seulement sur main après merge ou par Actions › Windows › Run workflow (résultats dans le résumé du run)
cd src-tauri && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo audit
cd src-tauri && cargo clippy -p safety --target x86_64-pc-windows-msvc --all-targets   # depuis Linux : vérifie safety::win (blake3 en feature « pure » le temps de la commande)
cd src-tauri && cargo clippy -p scan -p launchers --target x86_64-pc-windows-msvc --all-targets   # idem pour le scanner et les launchers
```

Conteneur Linux de dev : typecheck, lint, tests, build Vite, `cargo clippy/test` ; l'appli Windows (`npm run tauri dev`, WebView2, API Windows) ne tourne que sur la machine de l'utilisateur et en CI (`.github/workflows/windows.yml`). Code Windows derrière `#[cfg(windows)]`.
