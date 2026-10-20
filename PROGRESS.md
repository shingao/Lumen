# Lumen — avancement

Architecture : `docs/PLAN.md` (validée le 20/10/2026). Un jalon à la fois, arrêt après chacun pour test.

| # | Jalon | État |
|---|---|---|
| 1 | Projet Tauri, UI statique fidèle à la maquette (données mockées), navigation, thèmes, Paramètres et palette en squelette | **fait — validé** |
| 2 | Module `safety/` complet + tests (bloquants en CI) | **fait — validé** |
| 3 | Scanner Rust (parcours parallèle puis MFT), progression, sélecteur de disque réel | **fait — à tester** |
| 4 | Moteur de règles, recommandations et KPI calculés | à faire |
| 5 | Scripts, copie, exécution (simulation, confirmation, journal) | à faire |
| 6 | Nettoyage avancé : dormants, doublons, gros fichiers, gouffres système | à faire |
| 7 | Snapshots et Timeline | à faire |
| 8 | Dev et Jeux | à faire |
| 9 | Déplacement intelligent, recherche instantanée, export de rapport | à faire |
| 10 | Onboarding et astuces contextuelles | à faire |
| 11 | Intégration Windows (menu contextuel, zone de notification, alertes) et installeur | à faire |

---

## Jalon 3 — scanner

### Livré

- Crate `src-tauri/scan/` (membre du workspace) :
  - **Métadonnées uniquement.** Sous Windows, chaque dossier est lu par `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` sur le handle du dossier (ouvert avec `FILE_FLAG_OPEN_NO_RECALL`, sans suivre les liens) : file ID 128 bits, attributs et étiquette de point d'analyse de chaque entrée. Les tailles et dates de cette liste sont une copie que NTFS ne tient pas toujours à jour (voir Mesures) : chaque fichier est donc ouvert **par son file ID, pour ses attributs seulement** (`OpenFileById`, `FILE_READ_ATTRIBUTES`, `FILE_FLAG_OPEN_NO_RECALL` : ni lecture du contenu, ni rappel cloud, aucun conflit avec un fichier verrouillé) et ses tailles et dates sont lues dans son propre enregistrement. Fichiers cloud et points d'analyse jamais ouverts. Repli `FileIdBothDirectoryInfo` pour les systèmes de fichiers qui ignorent la classe étendue. Les fichiers compressés, clairsemés, WOF ou dédupliqués passent en plus par `GetCompressedFileSizeW`.
  - **Cloud (OneDrive / Files On-Demand).** `RECALL_ON_DATA_ACCESS` ou `OFFLINE` : fichier compté à part (« dans le cloud »), hors taille sur disque, jamais ouvert. Dossier `RECALL_ON_OPEN` (pas encore peuplé) : jamais énuméré, car l'énumérer ferait télécharger sa liste.
  - **Taille allouée** par défaut (compression, fichiers clairsemés ; petits fichiers résidents dans la MFT = 0 octet, comme l'Explorateur), **taille logique** en infobulle.
  - **Liens physiques** : chaque fichier compté une fois, dédoublonné par file ID après le parcours ; le lien retenu est celui au plus petit chemin (résultat stable malgré le parcours parallèle).
  - **Points d'analyse** (jonctions, liens symboliques, alias d'applis, liens WSL) jamais traversés, comptés à part ; fichiers WOF / dédupliqués comptés comme fichiers. Accès refusés et verrous comptés (avec échantillon de chemins), jamais bloquants.
  - **Chaque entrée** garde son file ID, sa taille, sa taille allouée, sa date de modification et de dernier accès (FILETIME, même unité que `safety`) : c'est ce que `safety` comparera pour l'anti-TOCTOU.
  - **Arène compacte** (72 octets par entrée + nom), enfants contigus, agrégation en un passage ; le front ne reçoit que des **tranches agrégées** (2 niveaux, 30 plus gros enfants + un nœud « autres »), rechargées au zoom.
  - **Parcours parallèle** (pool rayon dédié, un travail par dossier), **progression ~10/s** (fichiers, dossiers, Go, erreurs, durée, et les dossiers de premier niveau déjà comptés pour la carte en direct), **annulation** propre.
  - **MFT** (`scan::win::mft`) : lecture brute du volume (`\\.\C:`, `FSCTL_GET_NTFS_VOLUME_DATA`, extents de `$MFT`, correction des secteurs, `$STANDARD_INFORMATION`, `$FILE_NAME` — noms 8.3 ignorés, liens physiques = plusieurs noms —, `$DATA` résident / non résident / compressé / clairsemé, flux `WofCompressedData`, `$REPARSE_POINT`), enregistrements d'extension fusionnés, métadonnées NTFS (enregistrements < 24, `$Extend`) exclues, même classement et même règle de taille que le parcours. Le cache est d'abord écrit sur disque (`FlushFileBuffers` du volume) pour lire une MFT à jour.
  - **Élévation** : jamais d'UAC au démarrage. La MFT n'est lue que si l'appli est déjà administrateur, ou après un clic sur « Scan rapide (admin) » et une phrase d'explication : Lumen se relance lui-même avec `runas` en **assistant** (`--lumen-mft-scan <lecteur> <jeton> <dossier>`, arguments validés), qui lit la MFT et renvoie le résultat par un tube nommé (première instance, client local uniquement) ; le flux est validé au décodage (indices, bornes). Volumes non NTFS (exFAT, FAT32, réseau) : parcours.
- **Appli** : commandes typées `app_info`, `volumes_list`, `scan_start` (cible « dossier utilisateur » ou une lettre présente dans la liste des volumes — jamais un chemin), `scan_cancel`, `scan_summary`, `scan_tree` ; événements `scan:progress`, `scan:done`, `scan:failed`. Une seule analyse gardée en mémoire.
- **Interface** (même DA) : analyse du dossier utilisateur au lancement ; menu « Que faut-il analyser ? » (dossier utilisateur, chaque disque avec son espace libre) ; « Analyser à nouveau » ; « Scan rapide (admin) » avec sa bulle d'explication ; carte de progression (compteurs mono, barre, Annuler) et carte en direct des dossiers de premier niveau pendant la première analyse ; infobulle avec taille sur disque **et** taille logique ; tuiles « N autres éléments » ; ligne de bilan sous la légende (fichiers, cloud non comptés, liens physiques, liens non suivis, inaccessibles, méthode et durée). KPI réels : espace libre, « non analysé » = espace utilisé du disque − taille analysée ; « sans risque » et « à vérifier » attendent le moteur de règles (jalon 4). Couleurs de catégorie provisoires d'après les noms de dossiers (`core/categories.ts`), en attendant les règles.
- **Navigateur** (`npm run dev`) : le backend factice simule une analyse (progression puis résultat) sur les données de la maquette.
- **Tests** : 16 tests portables du scanner (Linux : liens physiques, liens symboliques, fichiers clairsemés, identité pour `safety`, dossier illisible, progression et annulation, tranches, sérialisation et rejet d'un flux falsifié) ; tests Windows sur de vraies fixtures (`scan/src/win/tests.rs`) : parcours (fichier verrouillé listé sans erreur, résident = 0, clairsemé, compressé, flux nommé ignoré, jonction non traversée, hors ligne compté à part, file ID), **MFT = parcours fichier par fichier** (mêmes fichiers, mêmes tailles allouées et logiques, mêmes liens, mêmes file ID), dossier interdit compté.
- **CI** : job `scan` (Windows, administrateur) : tests, puis banc d'essai — génération de 200 000 fichiers, parcours et MFT sur ce dossier, puis scan complet de `C:\` par chaque méthode dans un processus séparé avec le **pic de mémoire** ; tout est écrit dans le résumé du run.

### Mesures (CI Windows)

_À compléter avec le run de la PR._

### Décisions

- **Parcours maison plutôt que jwalk** : jwalk s'appuie sur `std::fs::read_dir`, qui ne donne ni file ID ni taille allouée ; il faudrait ouvrir chaque fichier pour les obtenir. La lecture par handle de dossier donne le file ID ; les tailles viennent de l'enregistrement de chaque fichier, ouvert par ce file ID (sans analyse de chemin). Le parcours reste parallèle (rayon, comme jwalk) et sert de repli pour tout volume non NTFS.
- La date de dernière utilisation vient de `LastAccess` seulement si Windows la tient à jour (`NtfsDisableLastAccessUpdate`), sinon de `LastWrite` ; l'information est dans le résumé de l'analyse (affichée au jalon 6 avec les fichiers dormants).
- Les dates et tailles d'une liste de dossier peuvent être en retard sur un fichier en cours d'écriture : sans danger, `safety` refusera l'élément (identité changée).
- Persistance SQLite de l'index (et recherche) : jalon 9 ; snapshots : jalon 7.

### Test manuel (jalon 3)

1. `npm run tauri dev` : l'analyse de ton dossier utilisateur démarre seule, sans demande UAC ; les compteurs avancent (~10 fois par seconde) et la carte se remplit en direct.
2. « Annuler » pendant une analyse : elle s'arrête proprement, l'analyse précédente reste affichée.
3. Survoler une tuile : taille sur disque et taille logique ; zoomer dans un dossier, revenir par le fil d'Ariane.
4. Ouvrir le menu « C: » : choisir un disque entier ; vérifier les chiffres de la ligne de bilan (fichiers, cloud, liens, inaccessibles, durée).
5. Si tu as OneDrive avec des fichiers « disponibles en ligne » : ils apparaissent « dans le cloud, non comptés » et **ne se téléchargent pas** (l'icône nuage reste).
6. « Scan rapide (admin) » : la bulle explique pourquoi ; « Lancer » déclenche une seule demande UAC ; le résultat doit être identique au parcours (même nombre de fichiers, même taille à quelques fichiers en cours d'écriture près), en beaucoup moins de temps.
7. Refuser l'UAC : message d'échec, l'analyse précédente reste.
8. Gestionnaire des tâches pendant un scan de `C:` : noter la mémoire de Lumen (et de l'assistant pour le scan rapide).

---

## Jalon 2 — `safety/`

### Livré

- Crate `src-tauri/safety/` (membre du workspace Cargo), seul habilité à décider d'une suppression, d'un déplacement ou d'une commande. Séparation nette :
  - **logique pure** (Linux et Windows) : `path.rs` (chemins absolus normalisés : `\\?\`, `\\?\UNC\`, `..` résolus lexicalement, chemins relatifs / de périphérique / `\\?\Volume{…}` / flux `:` / noms réservés / composants finissant par un point ou une espace refusés, comparaison insensible à la casse façon NTFS) ; `denylist.rs` (liste du brief codée en dur, emplacements réels **et** littéraux sur `C:`, parents des zones protégées refusés aussi) ; `allow.rs` (règle : dossier entier ou contenu seul, âge minimum ; sélection : chemins exacts) ; `check.rs` (pipeline dans l'ordre : canonisation → liste blanche → denylist (+ SYSTEM, dossier git) → liens sur le chemin et chaque parent → anti-TOCTOU → verrou) ; `plan.rs` (plan dédoublonné, empreinte BLAKE3, simulation, confirmation, double confirmation du définitif) ; `script.rs` (PowerShell `-WhatIf` par défaut, apostrophes typographiques échappées, pas de `Remove-Item -Recurse` qui suivrait les jonctions sous PowerShell 5.1) ; `commands.rs` (liste fermée : npm, pnpm, yarn, pip, uv, gradle, dotnet, docker, ollama, wsl, DISM, powercfg, vssadmin `/oldest`, cleanmgr ; arguments en tableau, noms de modèles et lecteurs validés) ;
  - **Win32** (`safety::win`, seul endroit avec `unsafe`) : `WinProbe` (`CreateFileW` sans suivre les liens, `GetFinalPathNameByHandleW` pour les noms 8.3 et la casse, `FileIdInfo` 128 bits, attributs, verrou par ouverture sans partage), dossiers connus (`SHGetKnownFolderPath`), Corbeille via `IFileOperation` sur un fil STA dédié.
- **Uniquement en simulation** : `plan::execute` vérifie l'empreinte et la double confirmation puis refuse (`REAL_EXECUTION_ENABLED = false`, assertion de compilation). Aucune commande IPC n'expose `safety` : sans scan (jalon 3), il n'y a pas encore d'identifiants d'éléments à lui confier, et le front n'envoie jamais de chemin. Le bouton « Nettoyer » reste un aperçu.
- **Tests** : 52 tests portables (47 à la première version) (chaque entrée de la denylist, variantes de casse, `\\?\`, `PROGRA~1` via la sonde, jonction vers `C:\Windows`, `..`, chemins relatifs, TOCTOU, verrous, git, âge minimum, empreinte, scripts) + 20 tests **Windows réels** (17 à la première version) sur des fixtures créées dans `%TEMP%` (`win/tests.rs`) :
  - adversariaux : jonction dans `%TEMP%` vers `C:\Windows` (la jonction, un fichier à travers elle, même avec une règle couvrant tout le dossier) ; fichier remplacé entre le scan et l'action (même nom, même taille, même date → nouvel ID refusé) ; chemin de plus de 260 caractères avec espaces, Unicode et emoji (remontée par `..` jusqu'à Windows, hors zone, remplacé, à travers une jonction : tous refusés ; contrôle positif : le même chemin légitime est accepté) ;
  - fixtures : jonction sur un parent, liens symboliques fichier et dossier, noms courts 8.3 (et `C:\PROGRA~1` → refusé), `\\?\` et casse, chemins relatifs, file ID NTFS (lien physique = même ID), fichier verrouillé ignoré et compté, attribut SYSTEM, dossier git, dossiers connus réels, simulation qui ne touche à rien, Corbeille `IFileOperation` (le fichier quitte son dossier et la Corbeille compte un élément de plus).
- **CI** : job `safety` (Windows, 8.3 activé sur `C:`) puis `safety-portable` (Linux) ; tous les autres jobs en dépendent. `scripts/check-safety-tests.mjs` exige chaque test de `src-tauri/safety/required-tests.txt`, refuse tout `#[ignore]`, vérifie que le nombre exécuté = le nombre listé, et que seul `safety/` lève l'interdiction clippy des suppressions.
- Langue : le français est la langue de référence, et le repli quand Windows n'est ni en français ni en anglais.
- Ménage : branche `main` créée depuis `b85cf16` ; brouillon `lumen/` retiré de `shingao/Rhoda`.

### Compléments demandés à la validation

- **Corbeille sans destruction silencieuse** : pré-contrôle explicite du volume avant tout envoi (`bin.rs`, pur et testé partout ; lu par `safety::win::volume_facts`) — type de lecteur (`GetDriveTypeW` sur la vraie racine du volume, `GetVolumePathNameW`), lecteur SUBST (`QueryDosDeviceW` → `\??\…`), Corbeille désactivée pour le volume (`NukeOnDelete` du GUID de volume) ou par stratégie (`NoRecycleFiles`), taille maximale de la Corbeille (`MaxCapacity`). Refaite sur le chemin réel **et** sur le chemin demandé (un SUBST résout vers `C:`), dans `Plan::build` et encore juste avant `IFileOperation`. Les drapeaux restent `FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOF_WANTNUKEWARNING | …` : `FOF_NOCONFIRMATION` n'est jamais seul, `FOF_WANTNUKEWARNING` l'emporte si Windows voulait malgré tout détruire. Nouveau test Windows : un lecteur SUBST temporaire (`DefineDosDeviceW`) → refusé, fichier intact.
- **Profils sans reposer sur les noms** : les autres profils viennent du registre (`ProfileList\<SID>\ProfileImagePath`), le profil courant de `FOLDERID_Profile`, tous résolus en chemins longs (`GetLongPathNameW`) et comparés comme chemins ; un profil d'un autre compte est protégé où qu'il soit (même hors de `C:\Users`). Le chemin canonique passe aussi par `GetLongPathNameW`. Nouveau test Windows : le profil courant est dans ProfileList, chaque autre profil est refusé, le nom 8.3 du profil courant résout vers lui, et un dossier anodin déclaré comme profil d'un autre compte est refusé par le pipeline.

### Décisions

- `safety` est un crate à part dans le workspace : il ne dépend que de `thiserror`, `blake3` et `windows`, ce qui permet de le vérifier pour la cible Windows depuis Linux (`cargo clippy --target x86_64-pc-windows-msvc`).
- La denylist s'applique au **chemin réel** (après canonisation) : le chemin demandé peut contenir des noms courts 8.3 (`C:\Users\RUNNER~1`) qui fausseraient la comparaison avec le profil courant. Le chemin demandé reste contrôlé à l'étape des liens (chaque parent).
- Un nom 8.3 resté non développé après canonisation (`PROGRA~1`) est refusé.
- Anti-TOCTOU : un fichier doit avoir le même ID, la même taille et la même date ; un dossier le même ID (sa date bouge dès qu'un fichier y change) — son contenu sera revérifié élément par élément au jalon 5.
- Corbeille : `FOF_WANTNUKEWARNING` empêche toute destruction silencieuse si un élément ne peut pas aller à la Corbeille (trop gros, volume sans Corbeille).
- Commandes externes en simulation : écrites en commentaire dans le script (`-WhatIf` ne s'applique pas aux exécutables).
- Racines `$…` d'un volume refusées (métadonnées NTFS) ; un `$` plus profond (`$facture.pdf`) reste permis.

### Reste

- Brancher `safety` sur le scan (jalon 3, identifiants d'éléments) et les règles (jalon 4) ; exécution réelle, journal, restauration depuis la Corbeille, suivi des éléments recyclés et lancement des commandes (jalon 5).

### Test manuel (jalon 2)

Rien de visible dans l'interface à ce jalon. Pour vérifier sur ta machine :

1. `cd src-tauri && cargo test -p safety` : 47 tests portables + 17 tests Windows, aucun ignoré.
2. `node scripts/check-safety-tests.mjs` depuis la racine : « OK : tous les tests requis de safety/ ont été exécutés ».
3. Vérifier que la Corbeille contient `a recycler é.txt` après les tests (preuve que rien n'a été détruit définitivement), puis la vider si tu veux.
4. `npm run tauri dev` : l'appli se comporte comme au jalon 1 ; « Nettoyer » n'affiche toujours qu'un aperçu.

---

## Jalon 1 — fait

### Livré

- Projet Tauri 2 (`src-tauri/`) : fenêtre sans décorations natives (1440 × 960, min 960 × 640), capabilities limitées aux contrôles de fenêtre, CSP stricte, lints clippy `unwrap_used` / `expect_used` / `panic` en `deny`, `clippy.toml` qui interdit dès maintenant `remove_*`, `rename`, `copy` et `Command::new` (réservés au futur `safety/`), erreurs `thiserror` en français, icône dessinée d'après le logo de la maquette (`design/app-icon.svg`).
- Front React 19 + TS strict + Vite, CSS modules, tokens des deux thèmes dans `src/styles/tokens.css`, Inter et JetBrains Mono embarquées (aucune URL réseau dans `dist/`).
- **Barre de titre** : logo, « Lumen », recherche centrale (ouvre la palette, `Ctrl K`), bascule soleil/lune, Paramètres, contrôles de fenêtre 46 px (fermer en rouge Windows au survol), zone de déplacement, double-clic pour agrandir.
- **Navigation** : contrôle segmenté textuel sous la barre de titre (Analyse, Timeline, Nettoyage, Dev, Jeux, Journal).
- **Analyse** (chiffres de la maquette) :
  - KPI : libre après nettoyage (+ « +18,0 Go / 7 j » qui mène à la Timeline), sans risque, à vérifier, non analysé ;
  - Explorer : treemap squarified 2 niveaux (gap 3 px, rayons 6/5), hachures sur le sans risque, gains « −12,3 Go », « −17,2 Go ? », « jusqu'à −14,0 Go » ; coloration Catégorie / Nettoyage / **Âge** (5 paliers neutres) ; légende adaptée au mode ; infobulle de la maquette ; zoom au clic (fil d'Ariane cliquable) ; clic sur une tuile sans enfants qui porte une recommandation → son détail ; menu contextuel (Copier le chemin et Ajouter à la sélection actifs, les autres grisés avec leur jalon) ; accès clavier (Tab, Entrée, Maj+F10) ;
  - sélecteur de disque à gauche du fil d'Ariane (C:, D: avec un jeu de données Jeux/médias, lecteur réseau listé mais grisé) ;
  - Recommandations : deux groupes, cases (seuls les sans risque cochés), survol d'une ligne qui estompe le reste de la treemap, total sélectionné, « Tout sélectionner (sans risque) », « Copier le script » (vrai presse-papiers, `-WhatIf`), « Nettoyer » (aperçu : arrive au jalon 5) ;
  - Panneau de détail : statut, nom, chemin, taille, phrase, méta, fichiers concernés avec barres de proportion, script PowerShell numéroté avec « Simulation -WhatIf » cochée par défaut, note, Ouvrir dans l'Explorateur / Copier le script / Nettoyer · taille.
- **Timeline, Nettoyage (4 onglets), Dev, Jeux, Journal** : même conteneur et même en-tête, état vide qui annonce leur jalon.
- **Paramètres** (icône de la barre de titre, `Ctrl ,`) : thème Système / Clair / Sombre et langue Système / Français / English **fonctionnels** ; les autres réglages (dossiers exclus, seuil des dormants, taille des doublons, suppression définitive désactivée, snapshot quotidien, zone de notification + seuil, menu de l'Explorateur, tutoriel, dossier des logs) sont enregistrés mais n'agissent qu'avec leur jalon.
- **Palette `Ctrl K`** : commandes (nouveau scan, thèmes, aller à…, paramètres, copier le script, colorer par…) et recherche floue dans l'index factice (nom, chemin, taille, date), navigation ↑ ↓ Entrée Échap.
- **Thème** : Système suit Windows en direct, pas de flash au démarrage (`public/theme-boot.js`), transition 150 ms coupée par `prefers-reduced-motion`. Raccourci `Ctrl Maj L`.
- **Langue** : FR/EN d'après la langue de Windows ; unités en base 1024 (`13,4 Go` / `13.4 GB`), nombres et dates localisés.
- **Tests** (`npm run check`, 28 tests) : squarify, formats (base 1024, Go/GB, dates relatives, langue), recherche floue, **contrastes AA des deux thèmes** (noms, tailles et gains sur chaque couleur de catégorie, de statut et d'âge, avec et sans voile ; textes de l'interface ; boutons), composants Recommandations et Treemap (jsdom). Rust : clippy + test d'erreur.
- **CI** `.github/workflows/windows.yml` : check + `npm audit`, clippy + tests + `cargo audit`, installeur NSIS en artefact, étape de signature Authenticode prête (secrets `LUMEN_SIGN_CERT` / `LUMEN_SIGN_PASSWORD`, sautée sans eux).
- Captures du jalon : `design/captures/jalon-1/` (clair, sombre, détail, Âge, anglais, Paramètres, palette, fenêtre étroite).

### Décisions

- Dépôt dédié `shingao/lumen` ; boutons « Nettoyer » vert dans les deux thèmes, « Copier le script » noir inversé en sombre ; unités « Go » (fr) / « GB » (en) en base 1024 ; déplacement intelligent et classement des caches : voir `docs/PLAN.md` § 10.
- Les données passent par un contrat `Backend` (`src/ipc/backend.ts`) ; le backend factice (`src/ipc/mock/`) sert la maquette en FR et EN. Le backend Rust le remplacera sans toucher aux écrans.
- Treemap rendue en `div` positionnées (pas de canvas) : texte net, focus clavier, hachures CSS.
- Une tuile de projet de la maquette laisse la place à ses artefacts tout en affichant sa taille totale (`layoutSize`) : reproduit à l'identique pour les données factices ; le scanner réel imbriquera les artefacts dans leur projet.
- Labels sur fond `--bg-sunken` (onglets inactifs, ligne survolée) : `#78716C` n'y fait que 4,4:1, d'où le jeton `--text-label-sunken` (`#6B655F` en clair).
- Mode Âge : les deux paliers les plus anciens passent les textes secondaires au ton fort (`data-deep`) pour rester AA.
- Taille 0,8 Go de la maquette affichée « 819 Mo » : conséquence normale de la base 1024.
- Thème sombre : neutres de la maquette transposés (`--text-2…4`, `--bg-sunken`, voile des sous-tuiles qui assombrit au lieu d'éclaircir), deltas `#6FCF9E` / `#F0B45A`.

### Écarts avec la maquette

- La rangée de navigation ajoute ~50 px au-dessus des KPI (demandée par le brief, absente de la maquette).
- La disposition des tuiles est calculée par l'algorithme squarified : mêmes tuiles, mêmes tailles et mêmes couleurs, mais pas exactement les mêmes positions que la maquette dessinée à la main.
- Barre de titre : soleil/lune et Paramètres ajoutés avant les contrôles de fenêtre (demandés par le brief).
- Bascule de coloration : 3e choix « Âge » (demandé par le brief).

### Reste (prochains jalons)

- Tout ce qui touche au disque : actions du menu contextuel (Ouvrir, Explorateur, Déplacer), Nettoyer, Ouvrir le dossier des logs, Ajouter un dossier exclu, Relancer le tutoriel.
- `tauri-specta` pour générer les types TS depuis Rust (jalon 3, avec les premières commandes).

### Test manuel (jalon 1)

1. `npm install` puis `npm run tauri dev` : la fenêtre s'ouvre sans cadre Windows, la barre de titre permet de déplacer, réduire, agrandir (double-clic aussi) et fermer.
2. Comparer l'écran Analyse à `design/maquettes/01-principal.png` : KPI 118 / 44,2 / 61,5 / 142 Go, tuiles hachurées, légende, colonne Recommandations avec 43,4 Go sélectionnés · 5 éléments.
3. Survoler une tuile : anneau sombre + infobulle (chemin, taille, fichiers, dernière ouverture, statut). Clic droit : menu contextuel ; « Copier le chemin » copie bien.
4. Survoler une ligne de recommandation : le reste de la treemap s'estompe. Cliquer le chevron : panneau de détail (comparer à `02-detail.png`), décocher « Simulation -WhatIf » fait disparaître `-WhatIf` du script, « Copier le script » le copie.
5. Cliquer « Projets » : zoom, le fil d'Ariane affiche `C: \ Users \ Arthur \ Projets` ; cliquer « Arthur » pour revenir. Ouvrir le menu « C: » et choisir D:.
6. Basculer Catégorie / Nettoyage / Âge : couleurs et légende changent.
7. Bouton lune/soleil : passage clair ↔ sombre en douceur ; Paramètres › Thème « Système » suit Windows (changer le mode de Windows pendant que l'appli tourne).
8. Paramètres › Langue « English » : toute l'interface et les unités passent en anglais (GB, point décimal) ; revenir à « Système ».
9. `Ctrl K` : la palette s'ouvre ; taper « grad » trouve `.gradle` ; taper « sombre » puis Entrée bascule le thème ; Échap ferme.
10. Parcourir Timeline, Nettoyage, Dev, Jeux, Journal : même conteneur, état vide avec le numéro du jalon.
