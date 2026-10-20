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
- Fin de jalon : `npm run check` vert, `cargo clippy --all-targets -- -D warnings` et `cargo test` verts, captures comparées à la maquette (`npm run captures`), `PROGRESS.md` à jour (fait / reste / décisions / écarts), checklist de test manuel, commit, push.
- Branche : `claude/bold-mayer-llad39`. Commits en anglais, `milestone N: …`.

## Stack

- Tauri 2 + Rust ; React 19 + TypeScript strict + Vite ; **aucune** librairie de composants ni d'icônes (SVG maison dans `src/components/icons.tsx`).
- CSS fait main : CSS modules (`*.module.css`) + variables de `tokens.css`. Stylelint interdit hex/rgb/hsl, `ms`/`s`, tailles et graisses en dur hors `src/styles/tokens*.css`.
- Polices embarquées : Inter (variable) et JetBrains Mono via `@fontsource` (aucun CDN).
- Treemap squarified maison : `src/core/treemap/squarify.ts` (2 niveaux, gap 3 px).
- À venir : scan jwalk / MFT, SQLite (rusqlite) dans `%LOCALAPPDATA%\Lumen\`, BLAKE3.

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

- Interface FR/EN selon la langue de Windows (`detectLocale`), réglable (Système / Français / English). Textes uniquement dans `src/i18n/fr.ts` (référence) et `en.ts` (mêmes clés), via `useT()` / `currentT()`. Aucune chaîne visible en dur.
- Tailles en **base 1024** comme l'Explorateur : `13,4 Go` (fr, Ko/Mo/Go/To) ou `13.4 GB` (en). Signe moins U+2212. Tout passe par `src/core/format.ts`.

## Règles de sécurité (non négociables)

- Toute suppression, tout déplacement et toute commande passent par **un seul module Rust `safety/`**. Le front n'envoie jamais de chemin libre : seulement des ids d'éléments du scan, des ids de règles ou de sélections. `clippy.toml` interdit `std::fs::remove_*`, `rename`, `copy`, `Command::new` ailleurs.
- Avant chaque action, dans l'ordre : canonisation (`\\?\`, 8.3, casse, `..`) → liste blanche (règle ou sélection Dormants/Doublons) → denylist → aucun reparse point sur le chemin ni ses parents → anti-TOCTOU (file ID, taille, date de modif identiques au scan) → fichier non verrouillé (sinon ignoré et compté).
- Corbeille par défaut (`IFileOperation`, annulable). Suppression définitive : option désactivée par défaut, double confirmation (taille + nombre). Commande officielle préférée à `Remove-Item`. Simulation par défaut ; exécution réelle après confirmation montrant total et liste. Un « À vérifier » n'est jamais coché par défaut.
- Scripts générés : `-WhatIf` par défaut, chemins absolus validés par `safety/` uniquement, commentés.
- Denylist codée en dur : `C:\Windows`, `Program Files`, `Program Files (x86)`, `ProgramData` (seules les commandes officielles des gouffres système y touchent) ; racines de volumes, EFI, `\Boot`, `\EFI`, `bootmgr`, `BOOTNXT` ; `System Volume Information`, `$Recycle.Bin`, `$Extend`, tout `$*` NTFS ; `pagefile.sys`, `swapfile.sys`, `hiberfil.sys` ; ruches (`NTUSER.DAT*`, `UsrClass.dat*`, `System32\config`) ; profils `Default`, `Public` et autres utilisateurs ; attribut SYSTEM ; un dossier contenant `.git` n'est jamais supprimé en entier.
- Déplacement intelligent : copie → vérification **BLAKE3 obligatoire** → jonction créée **et testée** (lecture d'un fichier à travers elle) → suppression **définitive** de l'original, annoncée comme telle ; rollback possible avant la suppression ; « Annuler le déplacement » recopie depuis la cible et retire la jonction.
- Sans droits admin par défaut ; élévation (assistant séparé) seulement pour le scan MFT et les actions système, expliquée en une phrase.
- Tauri : capabilities minimales (`src-tauri/capabilities/main.json` : contrôles de fenêtre seulement), CSP stricte, aucune commande shell exposée, IPC typé et validé côté Rust. Commandes externes : liste fermée de binaires, arguments en tableau.
- **Aucun accès réseau, aucune télémétrie.**
- Aucun `unwrap()`/`expect()`/`panic!` en production (lints clippy `deny`). Erreurs `thiserror` avec message français (`src-tauri/src/error.rs`).
- Si un test de `safety/` échoue, le build est bloqué (job CI dédié, jalon 2).

## Structure

```
design/            maquette HTML (source de vérité), captures de référence, icône
docs/PLAN.md       architecture validée
src/
  app/             App, store (zustand), thème/langue, raccourcis, commandes palette, accès données
  components/      Segmented, Button, Toggle, Menu, Toast, StatusDot, icônes, style des popovers
  features/        titlebar, kpi, explorer (treemap, légende), recommendations, detail, screens, settings, palette
  core/            logique pure testée : format, squarify, fuzzy, contrast
  ipc/             backend.ts (contrat), client.ts (seul contact Tauri), mock/ (données de la maquette), types.ts
  i18n/            fr.ts, en.ts
  styles/          tokens.css, fonts.css, global.css (+ contrast.test.ts)
src-tauri/         Rust : lib.rs, error.rs ; safety/, scan/, rules/… arrivent jalon par jalon
scripts/           captures.mjs, make-icon.mjs
```

Dépendances : `core/` ne dépend de rien ; `features/` → `core/`, `ipc/`, `components/`, `app/` ; seul `ipc/client.ts` importe `@tauri-apps/*`.

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
cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo audit
```

Conteneur Linux de dev : typecheck, lint, tests, build Vite, `cargo clippy/test` ; l'appli Windows (`npm run tauri dev`, WebView2, API Windows) ne tourne que sur la machine de l'utilisateur et en CI (`.github/workflows/windows.yml`). Code Windows derrière `#[cfg(windows)]`.
