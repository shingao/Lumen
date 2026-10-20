# Lumen — avancement

Architecture : `docs/PLAN.md` (validée le 20/10/2026). Un jalon à la fois, arrêt après chacun pour test.

| # | Jalon | État |
|---|---|---|
| 1 | Projet Tauri, UI statique fidèle à la maquette (données mockées), navigation, thèmes, Paramètres et palette en squelette | **fait — à tester** |
| 2 | Module `safety/` complet + tests (bloquants en CI) | à faire |
| 3 | Scanner Rust (jwalk puis MFT), progression, sélecteur de disque réel | à faire |
| 4 | Moteur de règles, recommandations et KPI calculés | à faire |
| 5 | Scripts, copie, exécution (simulation, confirmation, journal) | à faire |
| 6 | Nettoyage avancé : dormants, doublons, gros fichiers, gouffres système | à faire |
| 7 | Snapshots et Timeline | à faire |
| 8 | Dev et Jeux | à faire |
| 9 | Déplacement intelligent, recherche instantanée, export de rapport | à faire |
| 10 | Onboarding et astuces contextuelles | à faire |
| 11 | Intégration Windows (menu contextuel, zone de notification, alertes) et installeur | à faire |

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
