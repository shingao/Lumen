# Lumen — avancement

Architecture : `docs/PLAN.md` (validée le 20/10/2026). Un jalon à la fois, arrêt après chacun pour test.

| # | Jalon | État |
|---|---|---|
| 1 | Projet Tauri, UI statique fidèle à la maquette (données mockées), navigation, thèmes, Paramètres et palette en squelette | **fait — validé** |
| 2 | Module `safety/` complet + tests (bloquants en CI) | **fait — validé** |
| 3 | Scanner Rust (parcours parallèle puis MFT), progression, sélecteur de disque réel | **fait — validé** (PR #1 mergée) |
| 4 | Moteur de règles, recommandations et KPI calculés | **fait — validé** (PR #2 mergée) |
| 5 | Scripts, copie, exécution (simulation, confirmation, journal) | **fait — validé** (PR #3 mergée, testé en Sandbox) |
| 6 | Nettoyage avancé : dormants, doublons, gros fichiers, gouffres système | **fait — validé en lecture seule** (PR #4 mergée) ; nettoyage réel **non vérifié en réel** (Sandbox), gouffres système **non vérifiés en réel** (VM) et désactivés par défaut |
| 7 | Snapshots et Timeline | **fait — validé** (testé sur PC ; PR #5) |
| 8 | Dev et Jeux | **fait — validé en lecture seule** (jeux et projets détectés sur PC ; PR #6) ; purge des artefacts, shaders, Docker **non vérifiés en réel** (Sandbox), compactage WSL **non vérifié en réel** (VM) et désactivé par défaut |
| 9 | Persistance des analyses, recherche instantanée, déplacement intelligent, export de rapport | **fait — validé** pour la persistance, la recherche et l'export (testés sur PC, anonymisation vérifiée ; PR #7) ; déplacement intelligent **non vérifié en réel** (VM avec un second disque) et **désactivé par défaut** (réglage Expérimental) |
| 10 | Onboarding et astuces contextuelles | **fait — validé** (testé sur PC : tutoriel, analyse en fond, Passer / Échap / flèches, astuces une seule fois, relance depuis les Paramètres ; tutoriel à la place de l'écran validé ; PR #8) |
| 11 | Intégration Windows (menu contextuel, zone de notification, alertes) et installeur | **fait — en attente de validation** ; builds en **0.9.x** jusqu'aux tests de la checklist de release |

---

## Jalon 11 — intégration Windows et installeur

### Livré

**« Analyser avec Lumen » dans l'Explorateur** (`src-tauri/src/integration/`, `src/app/integration.ts`)
- Entrée **classique** du menu contextuel sur les **dossiers**, les **lecteurs** et le **fond d'un dossier** : `HKCU\Software\Classes\{Directory, Directory\Background, Drive}\shell\Lumen`, commande `"…\lumen.exe" --lumen-open "%V"`, icône de Lumen. Sous Windows 11, elle est dans « Afficher plus d'options » (accepté pour la v1 ; le menu moderne est décrit plus bas).
- **Proposée à l'installation (cochée)**, désactivable dans **Paramètres › Windows** : le registre de l'utilisateur est la seule source (lu et écrit par `integration_state` / `integration_set_menu`), l'installeur écrit exactement les mêmes clés. Le libellé suit la langue de l'interface (réécrit au démarrage et quand la langue change).
- **Le chemin reçu est une entrée comme une autre** (`integration/launch.rs`, tests) : seules deux formes d'arguments sont comprises (`--lumen-open <dossier>`, `--lumen-background`), le reste est ignoré ; le chemin est **canonisé** par `safety::path` (`..`, `\\?\`, `/`, flux, noms réservés, caractères interdits refusés), son lecteur doit être dans la liste des volumes (fixe ou amovible ; **réseau refusé**), et il doit être un dossier. **Il n'est jamais exécuté ni passé à un shell** : il ne fait qu'ouvrir l'Explorer de Lumen sur ce dossier (`openFolder` : l'analyse de son disque affichée, gardée de la session ou enregistrée, sinon une nouvelle ; l'analyse du démarrage lui cède la place). Refus expliqué par un toast (« les partages réseau ne sont pas analysés »…).
- **Une seule instance** (`tauri-plugin-single-instance`, premier plugin) : un second lancement transmet ses arguments à l'instance ouverte (qui se montre et ouvre le dossier) puis s'arrête.

**Zone de notification et démarrage** — **désactivés par défaut**
- **Icône** (Paramètres › Arrière-plan) : infobulle « Lumen — 73,8 Go libres sur 476 Go (C:) » (disque principal, relue toutes les 10 min), clic gauche : ouvrir ; menu **Ouvrir Lumen / Analyser C: / Quitter**. Tant qu'elle est active, **fermer la fenêtre la cache** : Lumen continue, **snapshot quotidien compris** ; « Quitter » ferme vraiment.
- **« Lancer au démarrage de Windows »** : **entrée Run de HKCU** (`Lumen` = `"…\lumen.exe" --lumen-background`), **jamais un service ni une tâche planifiée**. Au démarrage : caché dans la zone de notification si l'icône est active, sinon fenêtre réduite (jamais invisible sans icône : la fenêtre se montre au bout de 20 s si l'interface ne répond pas).

**Notifications d'espace libre** — **désactivées par défaut** (`src/core/alerts.ts`, tests)
- Quand l'espace libre d'un disque local passe **sous le seuil (10 % par défaut ; 5, 10, 15 ou 20 %)**, notification Windows (toast) « Espace disque faible sur D: — Plus que 44,0 Go libres (5 %). » **Au plus une fois par jour et par disque**, le disque étant retenu par sa **clé stable (GUID)**, jamais par sa lettre (`settings.alertsSent`, jours passés oubliés). **Un clic ouvre l'Analyse du disque concerné.** Vérifié toutes les 10 min tant que Lumen tourne.

**Installeur NSIS** (`src-tauri/installer/installer.nsi` + `hooks.nsh`)
- **Pour tous les utilisateurs, dans Program Files** (requis par l'assistant MFT, qui refuse un exécutable modifiable sans droits d'administrateur).
- **Page « Options »** après le choix du dossier : ☑ « Analyser avec Lumen » dans l'Explorateur, ☑ raccourci Bureau, ☐ lancer au démarrage de Windows. Par-dessus une installation, les cases reprennent l'état en place. En silencieux : `/NOMENU`, `/NODESKTOP`, `/AUTOSTART`.
- **Désinstallation propre** : retire les clés du menu contextuel (seulement celles qui lancent Lumen), l'entrée Run, les raccourcis (menu Démarrer, Bureau) et les fichiers. **Elle demande s'il faut garder les données** : case « Supprimer aussi les données de Lumen (historique, journal, réglages) », **décochée** ; cochée, elle supprime `%LOCALAPPDATA%\Lumen` et les réglages de l'interface — jamais à travers une jonction.
- **Elle ne touche JAMAIS aux jonctions du déplacement intelligent** ni à leurs cibles : Lumen tient leur liste dans `%LOCALAPPDATA%\Lumen\jonctions.txt` (au démarrage et après chaque déplacement, confirmation ou annulation) ; s'il y en a, la désinstallation **les liste et prévient avant de continuer** (OK / Annuler, Annuler par défaut) et **garde les données de Lumen**, dont le journal sert à annuler ces déplacements.
- **Mise à jour par-dessus** : fichiers remplacés, réglages (WebView2), base SQLite et choix d'intégration gardés ; les migrations SQLite passent au lancement suivant (chacune testée depuis une base vide et depuis la version précédente, `journal.rs`).
- **Signature : placeholder documenté** (`docs/SIGNATURE.md`, `scripts/sign.ps1`) : avec les secrets `LUMEN_SIGN_CERT` / `LUMEN_SIGN_PASSWORD`, Tauri signe `lumen.exe`, le désinstalleur et l'installeur **avant** de les empaqueter ; sans eux, build non signé.
- **Test réel en CI** (`scripts/ci/installer-test.ps1`, job `build`) : installation silencieuse, clés HKCU, raccourcis, lancement, **instance unique** (second lancement transmis puis arrêté, argument piégé jamais exécuté), **mise à jour par-dessus** (base et réglages gardés), **désinstallation** (tout retiré, **jonction et contenu intacts, données gardées**), puis installation sans options.

**Finitions v1**
- **Paramètres › À propos** : version (**pas de mise à jour automatique** : l'appli est sans réseau, la version est affichée et c'est tout), licence, lien du dépôt (affiché et copiable, jamais ouvert par Lumen), **licences des composants tiers** : `THIRD_PARTY_NOTICES.txt` **généré** par `npm run notices` (**cargo-about** pour les 227 crates de l'exécutable Windows, sans dépendances de dev ni de compilation ; fichiers de licence des 7 paquets npm embarqués, polices comprises), **embarqué** dans l'interface (chargé à l'ouverture) ; la CI échoue s'il n'est pas à jour.
- **Journal de plantage local** dans `%LOCALAPPDATA%\Lumen\logs`, **jamais envoyé** : plantage Rust → `crash-<1..5>.log` (version, fil, emplacement, message, pile ; le plus ancien est réécrit, rien n'est supprimé) ; erreurs non rattrapées de l'interface → `interface.log` (remis à zéro au-delà de 256 Kio). Paramètres › Aide › « Ouvrir le dossier ».
- Réglages Expérimentaux (gouffres système, compactage WSL, déplacement intelligent) **toujours désactivés par défaut**.
- Version **0.9.0** (`package.json`, `Cargo.toml`, `tauri.conf.json`).

### Décisions

- **Menu contextuel et démarrage dans HKCU, même avec une installation pour tous les utilisateurs** : Lumen peut ainsi les activer et les retirer depuis les Paramètres sans droits d'administrateur, et l'installeur (élevé, le plus souvent sous le même compte) écrit les mêmes clés. La désinstallation retire celles du compte qui désinstalle ; celles d'un autre compte qui les aurait activées restent (inoffensives : la commande ne mène plus à rien) — à noter si plusieurs comptes utilisent Lumen.
- **Le registre est la seule source** pour le menu et le démarrage (pas de copie dans les réglages) : ce qu'affichent les Paramètres est toujours ce que fait Windows, y compris après l'installeur.
- **Démarrage indépendant de l'icône** : coché sans icône, Lumen démarre en fenêtre réduite ; avec l'icône, caché dans la zone de notification. Pas de couplage caché entre les deux réglages.
- **Fenêtre créée invisible**, montrée par Rust au lancement ordinaire (`visible: false`) : un démarrage discret ne fait pas clignoter de fenêtre.
- **Notifications construites par l'interface** (textes traduits, tailles en base 1024) et **affichées par Rust** (`tauri-winrt-notification`, identifiant `com.lumen.disk`, posé par l'installeur sur le raccourci du menu Démarrer : sans installation, pas de toast). La lettre transmise est revérifiée contre la liste des volumes.
- **Modèle NSIS de Tauri copié** (`installer.nsi`, tauri-bundler 2.10.3, ajouts marqués « LUMEN ») : une page d'options ne peut pas s'ajouter par les seuls hooks. **À resynchroniser à chaque mise à jour de `@tauri-apps/cli`** (comparer avec `src/bundle/windows/nsis/installer.nsi` de la version de tauri-bundler utilisée). Compilé localement avec makensis 3.09 (0 avertissement).
- **Raccourci Bureau** déplacé de la page de fin vers la page d'options ; la page de fin garde « Lancer Lumen ».
- **Données gardées s'il reste des jonctions**, même si la case est cochée : sans le journal, un déplacement ne s'annule plus depuis Lumen.
- **Licence de Lumen** : aucune n'est déclarée dans le dépôt ; À propos affiche « Tous droits réservés ». **À choisir avant la v1.0** (une ligne dans `AboutGroup.tsx`, `about.licenseValue`).
- **Liste des jonctions en UTF-16 LE sans BOM** : lue par `FileReadUTF16LE` de NSIS, chemins accentués compris (`Déplacés`).

### Menu contextuel moderne de Windows 11 (documenté, non implémenté)

Le menu compact de Windows 11 n'accepte pas les verbes `shell\…` du registre. Il faudrait :
1. un **gestionnaire COM `IExplorerCommand`** (DLL native, Rust `windows` ou C++), avec `GetTitle`, `GetIcon`, `GetState` (visible pour un dossier, un lecteur ou un fond de dossier) et `Invoke`, qui lancerait `lumen.exe --lumen-open "<dossier>"` exactement comme aujourd'hui (même validation à l'arrivée) ;
2. un **package MSIX « sparse »** (identité de package pour une appli non empaquetée) : `AppxManifest.xml` déclarant l'extension `windows.fileExplorerContextMenus` (`desktop4`/`desktop5`, `ItemType` `Directory`, `Directory\Background`, `Drive`) et le `com:SurrogateServer` du gestionnaire ;
3. ce package **signé** (le même certificat que l'installeur : sans signature, Windows refuse de l'enregistrer), enregistré à l'installation (`Add-AppxPackage -ExternalLocation <Program Files\Lumen>` ou `PackageManager.AddPackageByUriAsync`), retiré à la désinstallation ; l'exécutable porte alors l'identité dans son manifeste (`msix:identity`).
Coût : une DLL COM, un manifeste, la signature obligatoire et un test sur Windows 11 réel. À reconsidérer quand Lumen sera signé.

### Écarts avec la maquette

- La maquette n'a pas d'écran À propos ni de page d'installeur : le groupe « À propos » reprend exactement les lignes des Paramètres (libellé, phrase grise, contrôle à droite) ; la liste des licences s'ouvre dans la fenêtre modale existante (texte brut mono sur `--bg-sunken`, défilant). Les groupes « Windows » et « Aide » passent dans la colonne de gauche pour équilibrer les deux colonnes.
- L'icône de la zone de notification est celle de l'appli ; son menu est le menu natif de Windows (pas de style Lumen possible).

### Checklist de release (v1.0)

Tout ce qui est marqué **« non vérifié en réel »**, avec son environnement de test. **La v1.0 n'est étiquetée qu'après ces tests** ; d'ici là, les builds sont en **0.9.x** (non signés).

| # | Fonction (jalon) | Environnement | Ce qu'il faut vérifier |
|---|---|---|---|
| 1 | Nettoyage réel des fichiers dormants, doublons, gros fichiers (6) | **Windows Sandbox** | Corbeille, restauration depuis le Journal, un exemplaire de chaque doublon gardé |
| 2 | Gouffres système : DISM, hibernation, points de restauration, Windows.old (6) — *Expérimental* | **VM Hyper-V** avec checkpoint | une demande UAC par commande, point de restauration avant, le plus ancien gardé s'il en reste moins de 2 |
| 3 | Purge des artefacts de projets, caches de shaders, `docker image prune` (8) | **Windows Sandbox** | source et `.git` intacts ; rien si Steam tourne ; images étiquetées gardées |
| 4 | Compactage WSL / Docker (8) — *Expérimental* | **VM** (WSL installé) | `wsl --shutdown`, gain mesuré, `.vhdx` toujours là, distribution qui démarre |
| 5 | Déplacement intelligent (9) — *Expérimental* | **VM avec un second disque NTFS** | copie vérifiée, jonction, suppression après confirmation, « Annuler le déplacement », reprise après coupure |
| 6 | Installeur : page Options, mise à jour par-dessus une 0.9.x, désinstallation avec et sans « Supprimer les données » (11) | **Windows Sandbox** | cases appliquées, réglages gardés à la mise à jour, tout retiré à la désinstallation |
| 7 | Désinstallation avec une jonction du déplacement intelligent (11) | **VM avec un second disque** (après le point 5) | liste affichée, Annuler arrête tout, OK : jonction, cible et données intactes |
| 8 | Menu contextuel Windows 10 et Windows 11 (« Afficher plus d'options »), dossier / lecteur / fond (11) | **Windows Sandbox** (Windows 11) | Lumen s'ouvre sur le bon dossier, une seule instance ; partage réseau refusé |
| 9 | Zone de notification, fermeture qui cache, Quitter, démarrage avec Windows (11) | **VM** (redémarrages) | infobulle juste, snapshot quotidien en fond, aucune fenêtre au démarrage avec l'icône |
| 10 | Notifications d'espace libre et clic (11) | **VM** (petit disque virtuel presque plein) | une seule par jour et par disque, le clic ouvre l'Analyse de ce disque |
| 11 | Signature Authenticode (11) | **CI + Windows Sandbox** | installeur, désinstalleur et `lumen.exe` signés ; SmartScreen sans « Éditeur inconnu » |

Déjà vérifiés en CI Windows à chaque push (pas besoin de VM pour ceux-là) : suppression réelle sur faux profil (e2e), déplacement C: → D: sur disques virtuels, installation / instance unique / mise à jour / désinstallation silencieuses (`installer-test.ps1`).

### Test manuel (jalon 11) — Windows Sandbox (1 à 7), VM pour 8 et 9

1. Lance l'installeur (`lumen-installer` de la CI) : page **Options** avec trois cases (menu contextuel ☑, Bureau ☑, démarrage ☐) ; installe dans Program Files.
2. Clic droit sur un dossier (Windows 11 : « Afficher plus d'options ») › **Analyser avec Lumen** : Lumen s'ouvre sur ce dossier, dans l'analyse de son disque. Recommence Lumen ouvert : **la même fenêtre** se montre et change de dossier. Clic droit sur le fond d'un dossier et sur un lecteur dans « Ce PC » : idem.
3. Paramètres › Windows : décoche « Analyser avec Lumen » → l'entrée disparaît du menu ; recoche → elle revient. Passe l'interface en English : le libellé devient « Analyze with Lumen ».
4. Paramètres › Arrière-plan : active l'**icône** → infobulle avec l'espace libre de C: ; ferme la fenêtre → Lumen reste dans la zone de notification ; menu › **Analyser C:**, puis **Quitter**.
5. Active « Lancer au démarrage de Windows », déconnecte-toi et reconnecte-toi : Lumen est dans la zone de notification, sans fenêtre (sans l'icône : fenêtre réduite dans la barre des tâches).
6. Paramètres › À propos : version 0.9.0, « pas de mise à jour automatique », lien du dépôt copié, **Afficher** les licences (texte complet, défilant). Aide › Journal de plantage › Ouvrir le dossier.
7. Désinstalle (Paramètres de Windows › Applications) : case « Supprimer aussi les données de Lumen » décochée → `%LOCALAPPDATA%\Lumen` reste ; plus d'entrée de menu contextuel ni de démarrage ni de raccourci. Réinstalle par-dessus une installation en place : les cases reprennent l'état en place, les réglages sont gardés.
8. *(VM avec un petit disque presque plein)* Active les **notifications** avec un seuil au-dessus de l'espace libre : une notification « Espace disque faible sur E: » ; clic → l'Analyse de E: ; pas de seconde notification le même jour.
9. *(VM avec un second disque, après un déplacement intelligent)* Désinstalle : la liste des dossiers déplacés s'affiche et prévient ; Annuler → rien n'est désinstallé ; OK → la jonction, la cible et `%LOCALAPPDATA%\Lumen` sont intacts.

---

## Jalon 10 — onboarding et astuces contextuelles

### Livré

**Tutoriel de bienvenue** (`src/app/onboarding.ts`, `src/features/onboarding/`)
- **Au premier lancement, 4 écrans dans la DA** : fond de l'appli, un conteneur centré (rayon 16, bordure fine), une **mini-treemap animée** (mêmes tuiles, couleurs de catégorie, rayon 6 et espacement 3 px que l'Explorer ; disque d'exemple, pas les données de l'utilisateur), une phrase, un bouton :
  1. « Voici où passe ton espace disque. » — les tuiles apparaissent une à une ;
  2. « Les zones hachurées peuvent partir sans risque. » — les hachures apparaissent sur les trois caches, le reste passe au second plan ;
  3. « Coche ce que tu veux libérer, Lumen te montre exactement ce qu'il va faire. » — coches et pastille « 3 éléments cochés · 28,1 Go » ;
  4. « Rien n'est supprimé sans ton accord, et tout passe par la Corbeille. » — les tuiles cochées passent en pointillés, « 3 éléments vers la Corbeille ».
- **« Passer » toujours visible**, **Échap ferme**, **flèches ← →** pour naviguer, focus sur le bouton principal à chaque écran et gardé dans le tutoriel (Tab / Maj+Tab), `role="dialog"` modal, phrase annoncée (`aria-live`), « 2 / 4 » et points de progression. Pendant le tutoriel, les raccourcis globaux (Ctrl K, Paramètres…) sont inactifs.
- **La première analyse démarre en arrière-plan** (comme avant, au démarrage) pendant le tutoriel ; en bas du conteneur : « Ton analyse tourne déjà en arrière-plan : 128 400 fichiers… » puis « Ton analyse est prête. ». « Voir mon analyse » mène à l'écran Analyse, déjà rempli ou presque.
- Terminé, passé ou fermé : mémorisé (`settings.onboardingDone`), jamais rouvert seul. **Paramètres › Aide › « Relancer le tutoriel »**.

**Astuces contextuelles** (`src/features/tips/TipBubble.tsx`)
- À la **première visite de chaque écran** (Analyse, Timeline, Nettoyage, Dev, Jeux, Journal), une **bulle dans le style des infobulles de la treemap** (surface des popovers, pointe vers l'onglet de l'écran) avec une phrase utile ; **fermée en un clic** (sur la bulle ou ×), **jamais répétée** : mémorisée dès qu'elle s'affiche (`settings.tipsSeen`), même si l'écran est quitté sans la fermer. Jamais pendant le tutoriel ni dans les Paramètres.
- **Paramètres › Aide › « Réinitialiser les astuces »** (toast de confirmation).

**Accessibilité et DA**
- **`prefers-reduced-motion`** : aucune animation ni transition (règle globale) ; chaque écran affiche directement son état final (capture `57-bienvenue-3-mouvement-reduit`).
- **Contraste AA dans les deux thèmes** : nouvelle paire vérifiée par `contrast.test.ts` (noms et tailles sur les tuiles mises au second plan : un voile **sous** le texte plutôt qu'une opacité de la tuile entière) ; les autres paires (texte sur conteneur, pastille sans risque, couleurs de catégorie) étaient déjà couvertes.
- Textes en français (référence) et en anglais, mêmes clés. Durées par jetons (`--dur-onboard`, `--dur-stagger`).

**Tests** (vitest) : ouverture au premier lancement seulement ; **séquence complète** (4 écrans, Précédent, « Passer » visible à chaque écran, focus, fermeture mémorisée) ; **Passer** ; **clavier** (flèches, **Échap**) ; analyse en arrière-plan affichée ; **relance depuis les Paramètres** ; **astuce affichée une seule fois** (fermée en un clic, absente à la visite suivante), une par écran, retirée en changeant d'écran, jamais pendant le tutoriel, **Réinitialiser les astuces** (fonction et bouton).

### Décisions

- **Tutoriel affiché à la place de l'écran** (sous la barre de titre, qui garde ses boutons de fenêtre) plutôt qu'en fenêtre par-dessus : c'est « le fond de l'appli » demandé, et rien derrière ne peut prendre le focus.
- **Mini-treemap sur un disque d'exemple** : la vraie analyse n'est pas encore prête au premier écran ; les tailles d'exemple restent plausibles (Vidéos, Projets, caches npm et Gradle…).
- **Astuce mémorisée dès qu'elle s'affiche** (pas seulement à la fermeture) : « jamais répétée » même si l'utilisateur change d'écran sans la fermer.
- Astuce **ancrée à l'onglet de l'écran** dans la navigation : même place partout, ne cache jamais un bouton d'action.
- **Utilisateurs existants** : leurs réglages n'ont pas encore `onboardingDone`, ils voient donc le tutoriel une fois après la mise à jour (« Passer » le ferme pour de bon).

### Écarts avec la maquette

- La maquette n'a ni tutoriel ni astuces : ils reprennent sa DA (conteneur centré sur le fond de l'appli, tuiles et hachures de la treemap, pastille hachurée « sans risque », bouton primaire noir, liens soulignés, surface des infobulles). Captures : `design/captures/jalon-10/` (57 à 62).

### Test manuel (jalon 10)

1. Premier lancement (ou Paramètres › Aide › Relancer le tutoriel) : 4 écrans, la mini-treemap s'anime, les hachures apparaissent au 2e écran, les coches au 3e, les pointillés au 4e.
2. Au clavier seulement : → / ← changent d'écran, Tab reste dans le tutoriel, Entrée sur « Suivant », Échap ferme.
3. « Passer » est visible à chaque écran et ferme ; le tutoriel ne revient pas au lancement suivant.
4. Pendant le tutoriel, le compteur de fichiers de la première analyse avance ; à la fin, l'Analyse est prête (ou presque).
5. Windows › Accessibilité › Effets visuels › Effets d'animation désactivés : plus aucune animation, chaque écran s'affiche directement.
6. Thème sombre et clair : textes lisibles partout (tuiles au second plan comprises).
7. Première visite de chaque écran : une bulle sous son onglet ; un clic la ferme ; elle ne revient plus (même après relance de l'appli).
8. Paramètres › Aide › Réinitialiser les astuces : les bulles réapparaissent à la prochaine visite de chaque écran.
9. En anglais (Paramètres › Langue) : tutoriel et astuces traduits.

---

## Jalon 9 — persistance des analyses, recherche, déplacement intelligent, export

### Livré

**Persistance des analyses** (reportée du jalon 3 ; `src-tauri/src/persist.rs`, `scan/src/codec.rs`)
- La dernière analyse de chaque cible (dossier utilisateur, chaque disque) est **enregistrée dans la base de Lumen, compressée** (format binaire maison `LUMSCAN` v1, puis LZ4 ; table `saved_scans`, schéma v5) à la fin de chaque analyse. Au lancement, **l'appli s'ouvre aussitôt sur le dernier résultat** (« analysé il y a 3 h », bandeau « Dernière analyse enregistrée : les tailles ont pu changer depuis » et **« Réanalyser »**), sans relancer de scan ; une analyse dont le disque est absent, d'un autre profil ou d'un autre volume à la même lettre (clé du volume) n'est pas reprise.
- Une analyse illisible (base abîmée, autre version du format) est **oubliée, jamais une erreur** ; la taille annoncée est vérifiée avant toute allocation. Tests : aller-retour, troncature à chaque longueur, 2 000 corruptions aléatoires, longueur démesurée, enregistrement abîmé supprimé.
- **Paramètres › Données locales** : taille de la base, nombre d'analyses enregistrées et leur taille compressée ; **« Purger… »** supprime snapshots et analyses enregistrées (les journaux des nettoyages et des déplacements sont gardés).
- Tout reste local. Les file ID enregistrés **ne remplacent jamais le recontrôle de `safety`** au moment d'agir : un plan se construit sur une analyse, chaque élément repasse les six étapes avant d'être touché.

**Recherche instantanée (Ctrl K)** (`scan/src/search.rs`, `src/core/searchQuery.ts`)
- Sur l'index des **noms seulement** (jamais le contenu) de toutes les analyses en mémoire (enregistrées comprises) : noms repliés (casse, accents), masque de caractères par nom pour écarter vite, sous-séquence floue avec bonus (début de mot, suite contiguë, sous-chaîne), en parallèle par blocs de 65 536 noms. Index construit à la première recherche (≈ 40 Mio pour 1 million d'entrées).
- **Filtres** : `ext:mp4` (plusieurs : `ext:mp4,mkv`), `>1go` / `<500mo` (base 1024, `1,5go` accepté), `type:vidéo|image|audio|archive|disque|installeur|document|dossier`, `disque:D`, combinables entre eux et avec des mots ; affichés en pastilles sous le champ ; les commandes existantes restent proposées sans filtre.
- **Entrée** ouvre l'élément (appli par défaut ; **un programme n'est jamais lancé**, il est montré dans l'Explorateur — `safety::open`, liste des extensions exécutables), **Ctrl+Entrée** l'ouvre dans l'Explorateur, **Maj+Entrée** zoome dessus dans la treemap (autre disque : son analyse d'abord).
- **Banc** : `cargo run --release -p scan --example bench -- search 1000000` (1 million d'entrées synthétiques, 12 requêtes, échoue au-delà de 50 ms) ; localement la pire requête prend ≈ 10 ms. En CI dans le job `bench` (hors chemin bloquant : sur `main` après merge ou à la demande).

**Déplacement intelligent** (`safety/src/relocate.rs`, `safety/src/win/relocate.rs`, `src-tauri/src/moves.rs`, `src/features/move/`) — **non vérifié en réel**
- **Derrière le réglage Paramètres › Expérimental › « Déplacement intelligent » (`experimentalMove`), désactivé par défaut** tant qu'il n'a pas été testé en VM avec un second disque : sans lui, ni bouton « Déplacer… » ni commande de palette. Le journal des déplacements, l'avertissement de l'Analyse et « Annuler le déplacement » restent toujours disponibles pour un déplacement déjà fait.
- Depuis l'Explorer (dossier zoomé › **« Déplacer… »**) ou la palette. Le front n'envoie qu'un id d'analyse, un id de nœud et une lettre de disque ; la cible est calculée par `safety` : **`<X>:\Lumen\Déplacés\<lettre>\<chemin d'origine>`**.
- **Vers un autre disque fixe NTFS local uniquement** (pas de lecteur réseau, d'amovible ni de SUBST ; la source aussi en NTFS). **Tous les refus sont réunis et expliqués** : denylist (le dossier, un parent de zone protégée ou un élément qu'il contient : sauvegardes, secrets, `.git`…), **dossier de jeu** ou qui en contient un (« déplace-le avec son launcher »), **disques virtuels** `.vhdx` / `.vhd` / `.avhdx` (WSL, Docker), **OneDrive et tout dossier synchronisé** (variables OneDrive, comptes OneDrive, racines du gestionnaire de synchronisation de Windows, fichiers cloud), **liens** dans le dossier ou sur son chemin, **déjà déplacé**, **fichiers ouverts par un programme : le Restart Manager dit lequel** (« Ouvert par Discord : ferme Discord d'abord »), **espace libre de la cible avec une marge de 10 %**, cible existante, déplacement précédent inachevé, dossier vide.
- **Dossiers d'applis dans AppData** (Local, LocalLow, Roaming) : autorisés, mais **« À vérifier »** avec l'avertissement « l'appli peut ne plus se mettre à jour correctement ».
- Étapes, chacune avec **retour en arrière automatique** en cas d'échec : **recontrôle complet** (mêmes refus, même contenu : empreinte BLAKE3 du plan affiché) → **copie fidèle** (`CopyFileExW` : contenu, attributs, **flux alternatifs** ; puis dates de création / accès / écriture, **ACL et propriétaire** ; dossiers : attributs, dates, flux et ACL) → **vérification BLAKE3 de chaque fichier, flux nommés compris** (date de dernier accès gelée pendant la lecture) → original **mis de côté** (renommé à côté, même disque) → **jonction** créée (point de montage `\??\X:\…`) → **test de lecture à travers la jonction** (cible relue, empreintes comparées) → **seulement après confirmation explicite** (case « Je confirme la suppression définitive de l'original »), **suppression définitive** de l'original par handle (jamais à travers un lien), après avoir vérifié que la jonction mène toujours à la copie et que l'original n'a pas changé. Refuser = tout remettre comme avant.
- **« Annuler le déplacement »** (Journal › Déplacements) : recopie depuis la cible vers un dossier voisin, vérification BLAKE3, retrait de la jonction, remise en place du dossier, contrôle du contenu, puis **la copie de la cible part à la Corbeille** (jamais de suppression définitive ; Corbeille indisponible : la copie reste, c'est dit). Échec à n'importe quelle étape : la jonction et la copie restent intactes, la recopie partielle est retirée.
- **Journal des déplacements** (table `moves`) dès le début : état (en cours, déplacé, remis comme avant, échec, annulé), étape et message. **Un déplacement interrompu** (Lumen fermé pendant la copie ou avant la décision) **est remis en état** au prochain affichage, sans jamais supprimer l'original (`relocate::recover`).
- **Jonctions suivies** : disque cible absent, copie disparue ou jonction retirée → **avertissement en tête de l'écran Analyse** (« Un dossier déplacé par Lumen est inaccessible ») et pastille dans le Journal.
- **Tests** : logique pure sur un système de fichiers en mémoire avec pannes injectables (22 tests : déplacement complet, chaque refus, **panne à chaque étape puis retour en arrière vérifié**, retour en arrière impossible sans rien supprimer, confirmation refusée, test de lecture raté, cible pleine avant et pendant la copie, original modifié jamais supprimé, annulation et ses pannes, reprise d'un déplacement interrompu, suivi) ; tampons de jonction, ACL et noms de flux (4 tests). **Tests Windows réels C: → D:** (8 tests, CI) : déplacement complet **avec flux alternatifs (fichier et dossier), lecture seule + caché, dates, ACL protégée et droits effectifs du dossier**, puis annulation ; **panne réelle à chaque étape** (création de dossier, copie, métadonnées, copie altérée, renommage, jonction, mauvaise jonction) puis retour en arrière ; confirmation refusée et suppression ratée ; annulation ratée sans perte ; **cible pleine** ; **dossier verrouillé** (Restart Manager nomme le programme) ; refus réels (SUBST, même disque, jonction, `.vhdx`, Windows) ; suivi des jonctions. **Bout en bout** (`e2e::deplacement_reel_d_un_dossier_du_faux_profil`) : faux profil analysé, dossier déplacé depuis l'id de nœud, journalisé, second déplacement interrompu remis en état, annulation.

**Export de rapport** (`src/core/report.ts`, `src/core/anonymize.ts`, `src/app/report.ts`, `src-tauri/src/report.rs`)
- **Page HTML autonome dans la DA** : tokens de Lumen, **Inter et JetBrains Mono embarquées** (woff2 en data URL), **aucune ressource externe ni script** (CSP `default-src 'none'`), thème courant ; **treemap statique** (2 niveaux, hachures du sans risque, légende), **KPI**, **recommandations**, **évolution de la Timeline** (courbe SVG de la série de la cible). Plus un **CSV** des éléments de l'analyse (`;` en français, `,` en anglais, UTF-8 avec BOM, **jamais une formule de tableur**).
- **« Anonymiser les chemins », coché par défaut** (à chaque ouverture) : `C:\Users\<nom>` → `%USERPROFILE%` (y compris la forme `~\…`), profils des autres comptes → `C:\Users\•••`, et **sous les dossiers personnels** (Documents, Bureau, Téléchargements, Images, Vidéos, Musique, OneDrive…) chaque nom devient `•••` (extension courte gardée : `•••.pdf`). Le pied de page le dit.
- Enregistré par Lumen **dans Téléchargements sous un nom neuf** (`Lumen-rapport-2026-10-24-2140.html`, `-2`… : **jamais d'écrasement**), puis « Afficher dans l'Explorateur » (le dernier rapport seulement). Le front n'envoie aucun chemin.

### Architecture par plateforme (portage macOS envisagé après la v1, rien d'implémenté pour Mac)

Ce qui est déjà derrière un trait ou un module par plateforme :
- `safety::probe::Probe` (sonde du système de fichiers) et `safety::exec::Effects` (Corbeille, définitif, commandes) — implémentations `safety::win::{WinProbe, WinEffects}` ;
- **nouveau** `safety::relocate::{MoveProbe, MoveEffects}` (déplacement) — `safety::win::relocate::WinMover` ;
- `scan::Source` (parcours) — `scan::win::WinSource`, MFT dans `scan::win` ;
- `launchers::Registry` (registre des launchers) — `WinRegistry` côté appli, `MemRegistry` en test ;
- côté appli, chaque domaine regroupe ses appels Windows dans un module `platform` (`scanning`, `cleanup`, `games`, `advanced/ipc`, `dev`, `timeline`, `moves`, `report`) avec une version « hors Windows » qui ne fait rien.

Endroits qui mélangent encore Win32 et logique commune (notés, pas de refactorisation maintenant) :
- **`#[cfg(windows)]` répandus dans les modules IPC** plutôt qu'un seul module plateforme : `moves.rs` (16), `scanning.rs` (13), `games/mod.rs` (13), `advanced/ipc.rs` (13), `cleanup.rs` (12), `report.rs` (4), `dev/mod.rs` (4), `timeline/mod.rs`, `analysis.rs`, `lib.rs` (2 chacun), `rules/fsread.rs` (2), `rules/git.rs` (1). Un trait `Platform` unique (volumes, dossiers connus, ouverture, sonde, effets) les remplacerait.
- **Attributs de fichiers Windows codés en dur dans la logique commune** : `ATTR_SYSTEM` (`rules/src/engine.rs`), `FILE_ATTRIBUTE_HIDDEN`… (`advanced/files.rs`), `ATTR_REPARSE` / `ATTR_RECALL_*` / `ATTR_COMPRESSED` (`scan/src/classify.rs`) : à remplacer par des drapeaux neutres (`hidden`, `system`, `link`, `cloud`) fournis par la source.
- **Chemins** : `safety::path::WinPath` (lettres, `\\?\`, noms réservés, repli de casse NTFS) sert de type de chemin partout, et la **denylist** liste des emplacements Windows (`C:\Windows`, `Program Files`, ruches, `$Recycle.Bin`…) : il faudrait un type de chemin par plateforme et une denylist macOS (`/System`, `/Library`, Time Machine…).
- **Règles** : cibles en variables Windows (`%LOCALAPPDATA%`, `%APPDATA%`, `%TEMP%`…) et commandes officielles Windows dans `rules.toml` ; un jeu de règles et de variables par plateforme.
- **Launchers** : chemins et clés de registre Windows dans chaque détecteur (Steam par `HKCU`, Epic par `ProgramData`…) ; Steam et Minecraft existent sur macOS avec d'autres emplacements.
- **Identité des volumes** (`timeline`) par GUID de volume Windows ; sur macOS ce serait l'UUID du volume.
- **Déplacement** : la logique (`relocate.rs`) est commune ; seule la jonction est propre à Windows (un lien symbolique jouerait ce rôle sur macOS, avec d'autres garde-fous).

### Décisions

- **Analyse enregistrée** : format binaire maison versionné plutôt que JSON (plus compact, décodage en une passe avec validation de chaque champ) ; une seule analyse par cible (la dernière), remplacée à chaque analyse.
- **Recherche** sur toutes les analyses en mémoire (pas seulement la cible affichée) : `disque:D` restreint ; l'index est construit à la demande et oublié avec l'analyse (même limite de 600 Mio).
- **Déplacement : un élément de la denylist dans le dossier le rend non déplaçable** (sauvegardes de jeux, secrets et stockage des navigateurs, `.git`…) : l'original étant supprimé définitivement, on applique la règle « jamais supprimés » à la lettre, même si la copie vérifiée existe. Conséquence : beaucoup de dossiers d'applis Electron (avec `Local Storage`) sont refusés ; à rediscuter si c'est trop strict.
- **Droits du dossier déplacé** : ses **droits effectifs** (entrées héritées comprises) sont figés en ACL protégée sur la copie, pour qu'un dossier du profil ne devienne pas lisible par les autres comptes sur le disque cible (qui hérite des droits de `D:\`). Sous-dossiers et fichiers : leur ACL telle quelle. À l'annulation, le dossier remis garde ces droits effectifs, sans réactiver l'héritage.
- **Dossiers créés sans modèle** (`CreateDirectoryW`) : `CreateDirectoryExW` crée le dossier puis échoue en recopiant les flux du modèle (vu en CI) ; attributs, dates, flux et ACL sont recopiés ensuite. La **compression NTFS** d'un dossier n'est donc pas reportée sur la copie (les fichiers gardent leurs attributs).
- **Dossiers ouverts** : le Restart Manager ne connaît que les fichiers ; un programme qui tient seulement le dossier (répertoire courant) fait échouer la mise de côté, et le retour en arrière remet tout en place avec le message de Windows.
- **Cible et marge** : la taille logique des fichiers plus 10 % (arrondi supérieur), espace libre lu pour l'utilisateur courant (quotas compris).
- **Rapport** construit côté interface (mêmes données, formats et traductions que l'écran), **enregistré côté Rust** dans Téléchargements : pas de boîte « Enregistrer sous » (aucun chemin libre venu du front, pas de dépendance de dialogue).
- **Rapport en thème courant** et police latine seulement (le français et l'anglais n'en demandent pas plus) : ≈ 120 Kio par rapport HTML.

### Écarts avec la maquette

- La maquette n'a ni dialogue de déplacement, ni export, ni rapport : ils reprennent la DA (surface des popovers sur voile comme la confirmation du nettoyage, résumé sur fond creux, chemins et tailles en mono, hachures vertes pour ce qui est vérifié, ocre « À vérifier » pour les avertissements, pastilles). Le rapport reprend les conteneurs, les KPI et les couleurs de catégorie de l'écran Analyse. Captures : `design/captures/jalon-9/` (47 à 56, dont deux rapports HTML rendus hors ligne, `55-rapport-html.html` et `56-rapport-html-sombre.html`).

### Test manuel (jalon 9) — sur PC pour la lecture, Windows Sandbox ou VM pour le déplacement

1. Ferme et relance Lumen : la dernière analyse s'affiche tout de suite avec « analysé il y a… » et le bandeau ; « Réanalyser » relance. Paramètres › Données locales : les analyses enregistrées et leur taille ; « Purger… » les vide.
2. Ctrl K : `ext:mp4 >1go`, `type:vidéo disque:D`, `type:dossier node_modules` ; les résultats arrivent sans attendre. Entrée ouvre (un `.exe` est montré dans l'Explorateur, jamais lancé), Ctrl+Entrée montre dans l'Explorateur, Maj+Entrée zoome dans la treemap.
3. Exporter › Page HTML (anonymisé) : le fichier est dans Téléchargements ; ouvre-le **hors ligne** : treemap, KPI, recommandations, courbe ; ni ton nom de compte ni les noms de tes documents n'apparaissent. Décoche « Anonymiser » : les vrais chemins. CSV : s'ouvre dans Excel avec les accents.
4. *(Sandbox / VM avec un second disque NTFS)* Paramètres › Expérimental › Déplacement intelligent (désactivé par défaut : sans lui, pas de bouton « Déplacer… »). Zoome sur un dossier de quelques Go › Déplacer… : taille, cible `D:\Lumen\Déplacés\…`, espace nécessaire (+10 %).
5. Ouvre un fichier du dossier dans une appli qui le verrouille (Word, un lecteur vidéo…) : refus « Ouvert par … : ferme … d'abord ». Ferme-la, revérifie.
6. Copier et vérifier : progression (copie, vérification BLAKE3, jonction, test) ; à la fin, l'original est encore là. « Tout remettre comme avant » : rien n'a changé (pas de jonction, pas de copie sur D:).
7. Recommence et confirme la suppression : l'espace est libéré sur C: ; le dossier s'ouvre toujours au même chemin (icône de raccourci dans l'Explorateur) ; un fichier avec un flux `Zone.Identifier` (fichier téléchargé) garde son avertissement ; dates et droits (Propriétés › Sécurité) identiques.
8. Débranche le disque cible (ou change sa lettre) puis relance Lumen : avertissement en tête de l'Analyse et pastille « Disque cible absent » dans le Journal.
9. Rebranche-le › Journal › Déplacements › « Annuler le déplacement » : le dossier revient à sa place (plus de jonction), la copie de D: est dans la Corbeille.
10. Refus attendus : un dossier OneDrive, un dossier de jeu Steam, `AppData\Local\Docker` (`.vhdx`), un dossier contenant une sauvegarde de jeu, vers le même disque ou une clé USB.
11. Ferme Lumen de force (Gestionnaire des tâches) pendant la copie d'un gros dossier, relance, ouvre le Journal : le déplacement est « Remis comme avant », l'original est intact, rien ne reste sur D:.

---

## Jalon 8 — Dev et Jeux

### Livré

**Général**
- **Dernière analyse de chaque disque gardée pendant la session** (note du jalon 6) : `scanning.rs` garde jusqu'à 4 analyses, une par cible (la plus ancienne oubliée d'abord) ; revenir à un autre disque par ← / → ou par le sélecteur réaffiche son analyse **sans la relancer** (`scan_cached`, `showCachedScan`). Test Rust (`derniere_analyse_de_chaque_disque_gardee`) et test d'interface (← vers l'autre disque ne lance aucune analyse).
- **Lecture tolérante** de tout fichier de launcher ou de projet (crate `src-tauri/launchers/`, `dev/mod.rs`) : fichier absent, illisible, verrouillé, trop gros (8 Mio au plus) ou mal formé = élément ignoré et **compté** (« 2 fichiers de launchers ignorés », avec la liste et la raison), jamais une erreur qui arrête la détection. Parseur VDF maison tolérant (erreurs `Truncated` / `Malformed` / `TooDeep`, profondeur bornée). Tests sur fichiers corrompus : VDF tronqué, ACF sans `installdir`, JSON Epic tronqué ou d'un autre format, `localconfig.vdf` illisible, manifeste verrouillé (Windows), `package.json` / `Cargo.toml` / `pyproject.toml` tronqués ou binaires.
- **Aucun accès réseau** : jaquettes lues seulement dans le cache local des launchers (Steam : `appcache\librarycache`, 2 Mio au plus, servies en data URL) ; sans jaquette locale, une vignette à l'initiale du jeu.

**Jeux** (`src-tauri/launchers/`, `src-tauri/src/games/`, `src/features/games/`)
- Détection **en lecture seule** : Steam (chemin par `HKCU\Software\Valve\Steam`, bibliothèques par `libraryfolders.vdf` — ancien et nouveau format, plusieurs disques —, jeux par `appmanifest_*.acf`, dernière partie par `userdata\<id>\config\localconfig.vdf` et `LastPlayed` des manifestes, outils comme Steamworks Common Redistributables écartés) ; Epic (`ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`, DLC et installations incomplètes écartées) ; GOG (`HKLM\SOFTWARE\GOG.com\Games`), Ubisoft Connect (`Ubisoft\Launcher\Installs`) ; Battle.net, EA app et repli général par les clés **Uninstall** (entrées « Steam App » écartées pour ne pas compter deux fois, doublons fusionnés par dossier d'installation). Registre lu dans les deux vues (64 et 32 bits).
- Par jeu : jaquette locale, **taille** (annoncée par le launcher, sinon **mesurée** sur le disque et marquée « mesurée »), **disque**, **dernière partie si le launcher la connaît, sinon « inconnue »** — jamais une date inventée. Statut **« À garder »**, **« À vérifier »** s'il n'a pas été lancé depuis plus de 6 mois (183 jours). **Aucune case à cocher** sur un jeu. Tri par taille, dernière partie (inconnue en dernier) ou nom.
- **Désinstaller / Déplacer : Lumen ne supprime ni ne déplace jamais un jeu.** Liste fermée dans `safety::launch` : `steam://uninstall/<appid>` pour Steam, `steam://open/settings` (gestionnaire de stockage) pour déplacer un jeu Steam, et pour les autres l'exécutable du launcher, seulement s'il est sous Program Files, n'est pas un lien et porte un nom connu (EpicGamesLauncher.exe, GalaxyClient.exe, Battle.net.exe, EADesktop.exe, UbisoftConnect.exe…). Launcher introuvable : boutons grisés, « désinstalle ce jeu depuis Paramètres › Applications ». Jamais notre déplacement intelligent sur un dossier de jeu.
- **Caches de shaders** (onglet « Caches de shaders ») : NVIDIA DXCache / GLCache, AMD, D3DSCache, mesurés dans l'analyse affichée ; « À vérifier », **jamais cochés**, explication (recompilation, premiers lancements plus longs, saccades temporaires) et conseil (limiter la taille du cache dans le Panneau de configuration NVIDIA / AMD Software). **Nettoyage refusé si un jeu ou un launcher tourne** : `safety::guard::ProcessGuard` (processus des launchers + exécutables sous les dossiers d'installation des jeux) inscrit dans le plan et dans son empreinte ; `exec::run` relit la liste des processus au moment de l'action et n'exécute rien si l'un tourne (« Steam est ouvert : ferme-le avant de nettoyer, rien n'a été fait ») ; l'interface prévient avant et grise la case. La règle `caches-shaders` reste refusée sur l'écran Analyse. Le **shadercache de Steam** (sous Program Files) est **informatif** : taille par bibliothèque et « gère-le depuis Steam ».

**Dev** (`src-tauri/src/dev/`, `src/features/dev/`)
- **Projets** détectés dans l'analyse affichée par `.git`, `package.json`, `Cargo.toml`, `pyproject.toml`, `build.gradle(.kts)`, `*.csproj`, `go.mod` (le projet le plus haut seulement ; jamais dans AppData, les dossiers cachés ni `node_modules`). Par projet : **nom** (lu dans le manifeste, sinon le dossier), **langage**, **dernier commit** (lecture de `.git` du jalon 4, sans lancer git ; sinon dernier fichier modifié, dit « modifié il y a… »), **taille du source**, taille de `.git` (en infobulle) et **taille des artefacts régénérables**. Tri par **« Inactif depuis »** et par **taille d'artefacts**.
- **« Purger les artefacts »** par projet ou par sélection (cases par artefact) : seulement les nœuds couverts par les règles `artefacts-projets-actifs` (sans risque) / `artefacts-projets-inactifs` (à vérifier) — `node_modules` seulement avec un fichier de verrouillage, `target`, `.venv`, `build`, `.next`, `.gradle`… ; **jamais le source ni `.git`** (identifiants refusés par `select_artifacts` s'ils ne sont pas des artefacts couverts) ; toujours le plan `safety` (6 étapes, Corbeille par défaut, journal).
- **Caches globaux** : recommandations de la catégorie « Caches de paquets » (npm, pnpm, Yarn, pip, uv, NuGet, Gradle par leurs commandes officielles), classement du jalon 4 (registre Cargo et `.m2` « À vérifier ») ; même confirmation que l'écran Analyse.
- **Docker** : `docker image ls --filter dangling=true` (lecture seule, liste fermée de `safety`) ; liste des images concernées (identifiant, âge, taille) ; « Supprimer ces images » → `docker image prune --force` **seulement si Docker répond**, après la confirmation habituelle. Docker absent ou arrêté : dit en une phrase, rien n'est proposé.
- **Compactage WSL / Docker** (reporté du jalon 4), **derrière le réglage Expérimental** « Compactage WSL / Docker » (désactivé par défaut, à part des gouffres système) : distributions (`HKCU\…\Lxss`, `BasePath` + `VhdFileName`) et disques de Docker Desktop (`docker_data.vhdx`, `ext4.vhdx`) avec leur taille ; **avertissement : `wsl --shutdown` arrête tous les processus Linux et Docker en cours** (à l'écran et dans la confirmation) ; plan = `wsl --shutdown` (prérequis obligatoire) puis, par disque, `OfficialCommand::VhdCompact` : script PowerShell **élevé lancé directement par `ShellExecuteEx` runas** (jamais par l'assistant MFT) qui revérifie que c'est un `.vhdx` ordinaire (pas un lien), puis **`Optimize-VHD` sur le disque monté en lecture seule** si le module Hyper-V est là, sinon **diskpart `attach vdisk readonly` / `compact vdisk` / `detach vdisk`**. **Jamais de suppression d'un vhdx** (le seul `Remove-Item` du script vise son fichier diskpart temporaire). Gain mesuré par l'espace libre avant / après et journalisé. Pas de point de restauration pour ces commandes.

**Tests**
- Fixtures de launchers (`launchers/src/tests.rs`) : machine complète avec **bibliothèques Steam sur deux disques**, manifests Epic (jeu, DLC, installation incomplète), GOG, Ubisoft, Battle.net / EA par Uninstall, **fichiers corrompus** ; fixtures de projets (`dev/tests.rs`) : avec et sans fichier de verrouillage, avec et sans git, actif et inactif depuis 400 jours, manifestes tronqués, outil dans AppData jamais pris pour un projet.
- `safety` : processus ouverts → rien n'est fait (`jeu_ou_launcher_ouvert_rien_n_est_fait`), Docker seulement les images listées, disque virtuel compacté jamais supprimé, launchers en liste fermée.
- **Bout en bout Windows** (`e2e::purge_des_artefacts_de_faux_projets`) : deux faux projets (web avec lockfile, `.git` avec HEAD / objets / refs, `node_modules`, `.next` ; Rust avec `target`) analysés, artefacts purgés par `dev::select_artifacts` + `safety` → **seuls les artefacts partent à la Corbeille, le source et `.git` sont identiques octet pour octet**.
- Interface (vitest) : jeux sans case, « inconnue », Désinstaller / Déplacer n'ouvrent que le launcher, shaders jamais cochés et bloqués si un jeu tourne, projets triés et purgés par identifiants, Cargo et `.m2` à vérifier, Docker arrêté, WSL derrière le réglage Expérimental.

### Compléments demandés à la validation (testé en lecture seule sur PC : jeux et projets détectés)

- **Launchers supplémentaires, en lecture seule, mêmes règles** (Lumen n'ouvre que le launcher) :
  - **Riot Client** : `%ProgramData%\Riot Games\Metadata\<produit>.<branche>\*.product_settings.yaml` (`product_install_full_path`), client exclu, exécutable lu dans `RiotClientInstalls.json` ; repli sur les clés Uninstall « Riot Game … » (ni Riot Client ni Vanguard).
  - **CurseForge / Minecraft** : chaque instance de `%USERPROFILE%\curseforge\minecraft\Instances` (ou l'ancien `Documents\Curse\Minecraft\Instances`) est un jeu, avec sa taille mesurée et sa dernière partie (`lastPlayed` de `minecraftinstance.json`, « 0001-01-01 » = inconnue) ; Minecraft: Java Edition (`%APPDATA%\.minecraft`, dernière partie = `lastUsed` des profils) et chaque installation du Minecraft Launcher qui a son propre dossier de jeu.
  - **Xbox / Microsoft Store** : `.GamingRoot` à la racine de chaque disque fixe (dossier `XboxGames` par défaut), `Content\MicrosoftGame.config` de chaque jeu (nom affiché, logo local) ; **affichés seulement**, « Désinstaller » et « Déplacer » ouvrent l'appli Xbox. **WindowsApps, WpSystem, XboxGames et .GamingRoot sont dans la denylist** (`StoreApps`) : jamais lus en contenu, jamais supprimables.
  - Fichiers absents, tronqués ou mal formés (YAML sans chemin, JSON d'instance tronqué, `.GamingRoot` corrompu, jeu Xbox sans configuration) : ignorés et comptés ; tests sur fixtures.
- **Données de jeu hors du dossier d'installation** (`Documents\My Games`, `Saved Games`, `AppData\LocalLow\*`, éditeurs connus de `AppData\Local` et `Roaming`, `curseforge`, `.minecraft`) : zone **« Jeux »** des règles, statut **« À garder »** (aucune règle ne les propose ; les dossiers non ludiques de LocalLow restent « Outils & autres »).
- **Sauvegardes dans la denylist de `safety`** (`GameSave`), où qu'elles soient : dossiers `Saves`, `Save`, `Savegame(s)`, `Save Games`, `Saved Games`, `SavedGames`, `SaveData`, `PlayerProfiles` et fichiers `.sav`, `.save`, `.lsv` — jamais proposées, jamais supprimables, même par Doublons, Dormants ou Gros fichiers (qui consultent la denylist), et un dossier nettoyé qui en contient les garde. Tests : `.sav` en double, dormant et gros jamais proposé (`advanced::saves_tests`), sauvegarde dans un dossier nettoyé laissée en place (`exec`), denylist, règles.
- **Onglet Jeux : « Données et sauvegardes : X Go (protégées) »** sous chaque jeu quand elles sont trouvées (nom du jeu ou de son dossier comparé aux dossiers de `My Games`, `Saved Games`, `AppData\Local|LocalLow|Roaming` — sous un éditeur, le début ou la fin du nom suffit ; `userdata\<compte>\<appid>` de Steam ; `saves` d'une instance Minecraft), dossiers en infobulle, taille mesurée sans lire le contenu.
- **Analyses gardées bornées en mémoire** : 600 Mio au plus au total (estimation : nœuds, noms, résultats des règles), **la moins récemment utilisée est oubliée** (chaque accès à une analyse la rend récente ; celle qui sert n'est jamais oubliée, même seule au-dessus de la limite). **Paramètres › Analyse › Analyses gardées en mémoire** : total, limite, détail par disque et jauge.

### Décisions

- **Riot Client et CurseForge s'installent hors de Program Files** (`C:\Riot Games`, `%LOCALAPPDATA%\Programs`), donc modifiables sans droits d'administrateur : `safety::launch::LauncherOpen::UserApp` ne les ouvre (nom d'exécutable en liste fermée, disque local, aucun lien sur le chemin) **que si Lumen n'est pas élevé** — sinon un programme modifiable par l'utilisateur hériterait des droits d'administrateur ; message « ouvre ce launcher toi-même ». Riot Client et CurseForge ne déplacent pas de jeu : « Déplacer » grisé, avec la raison.
- **Appli Xbox et Minecraft Launcher du Store** ouverts par leur identifiant d'appli figé (`shell:AppsFolder\Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App`, `…Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft`) ; l'ancien Minecraft Launcher sous Program Files est préféré s'il existe.
- **Sauvegardes protégées par leur nom**, de façon large (y compris `Save` et `SaveData`) : un refus de trop ne coûte rien, une sauvegarde perdue est irrécupérable. Les autres données de jeu (caches, journaux) restent « À garder » sans être dans la denylist.
- **Liens profonds des launchers autres que Steam non utilisés** (`com.epicgames.launcher://…`, `goggalaxy://…` : comportement non documenté ou variable selon les versions) : Lumen ouvre l'exécutable du launcher, l'utilisateur va dans sa bibliothèque. Pour Steam, `steam://uninstall/<appid>` et le gestionnaire de stockage (`steam://open/settings`).
- **Docker : seulement les images sans étiquette** (`image prune` sans `--all`) : les images étiquetées peuvent servir à un projet ; c'est la liste montrée qui part.
- **Jaquettes : seulement Steam** garde des jaquettes locales fiables (`librarycache`) ; Epic, GOG, Battle.net, EA et Ubisoft n'en ont pas de lisibles sans réseau → vignette à l'initiale (couleur « Jeux » de la treemap).
- **Taille d'un jeu** : celle du launcher si elle existe (Steam `SizeOnDisk`, Epic `InstallSize`, Uninstall `EstimatedSize`), sinon mesurée par le parcours du scanner sur son dossier (taille allouée).
- **Garde « jeu ouvert »** : vérifiée par `safety` au moment de l'action (pas seulement à l'écran), sur les noms de processus des launchers et tout exécutable sous un dossier de jeu détecté.
- **Réglage Expérimental séparé** pour le compactage WSL (`experimentalWsl`) : il se teste en VM indépendamment des gouffres système.
- **Projets cherchés dans l'analyse affichée** (pas de parcours supplémentaire du disque) : il faut analyser le dossier qui les contient.

### Écarts avec la maquette

- La maquette n'a pas d'écrans Jeux et Dev détaillés : ils reprennent sa DA (conteneur rayon 16, onglets segmentés, listes à bord fin, chiffres et chemins en mono, pastilles de statut, couleur « Jeux » pour les vignettes). Captures : `design/captures/jalon-8/` (33 à 44 ; 01 à 32 refaites).

### Test manuel (jalon 8) — sur PC pour la lecture, Windows Sandbox / VM pour ce qui supprime

1. Jeux : tes jeux Steam (sur plusieurs disques si tu en as), Epic, GOG, Battle.net, EA, Ubisoft apparaissent avec taille, disque, dernière partie (ou « inconnue ») ; aucun n'est coché ; un jeu pas lancé depuis 6 mois est « À vérifier ».
2. « Désinstaller » sur un jeu Steam ouvre la fenêtre de désinstallation de Steam ; « Déplacer » ouvre le gestionnaire de stockage ; sur un jeu Epic / GOG, le launcher s'ouvre. Rien n'est supprimé par Lumen.
3. Renomme un `appmanifest_*.acf` en le tronquant (copie de sauvegarde !) : le jeu disparaît, « 1 fichier de launchers ignoré » le signale, rien ne plante.
4. Caches de shaders (après une analyse de C:) : rien de coché ; Steam ouvert → avertissement et case grisée ; Steam fermé → nettoyage en Sandbox, puis le Journal le montre.
5. Dev : tes projets apparaissent avec langage, dernier commit, tailles ; tri « Inactif depuis » / « Artefacts ».
6. En Sandbox : « Purger » sur un projet → seuls `node_modules` / `target`… partent à la Corbeille ; le source et `.git` restent (vérifie `git status`).
7. Caches globaux : npm par sa commande officielle ; Cargo et `.m2` sont « À vérifier ».
8. Docker lancé : la liste des images sans étiquette s'affiche ; « Supprimer ces images » puis confirmation → `docker image ls` ne les montre plus, les images étiquetées sont restées. Docker arrêté : message, bouton grisé.
9. En VM : Paramètres › Expérimental › Compactage WSL / Docker ; une distribution cochée → confirmation avec `wsl --shutdown` et l'avertissement, une demande UAC, puis le gain mesuré dans le bilan et le Journal ; le `.vhdx` existe toujours et la distribution démarre.
10. ← vers un autre disque déjà analysé dans la session : l'analyse réapparaît tout de suite, sans relancer de scan.
11. Riot, CurseForge / Minecraft, Xbox : tes jeux apparaissent (instances CurseForge avec leur taille) ; « Désinstaller » ouvre le Riot Client, CurseForge ou l'appli Xbox ; « Déplacer » est grisé pour Riot et CurseForge. Lumen lancé en administrateur : Riot Client et CurseForge ne s'ouvrent pas, avec le message.
12. « Données et sauvegardes : X Go (protégées) » sous Baldur's Gate 3, Cyberpunk… (dossiers en infobulle). Nettoyage › Doublons / Dormants / Gros fichiers : aucun `.sav`, `.lsv` ni fichier d'un dossier `Saves` n'est proposé, même copié en double dans Documents.
13. Paramètres › Analyse › Analyses gardées en mémoire : la consommation augmente à chaque disque analysé et ne dépasse pas 600 Mo ; au-delà, le disque le moins récemment consulté doit être réanalysé.

---

## Jalon 7 — snapshots et Timeline

### Livré

- **Base SQLite, schéma v4** (`src-tauri/src/journal.rs`, même base que le journal : `%LOCALAPPDATA%\Lumen\lumen.db`) : migrations versionnées dans l'ordre (`MIGRATIONS`), chacune dans une transaction. **Testées depuis une base vide** (toutes les tables et colonnes présentes, version 4) **et depuis la version précédente** (base v3 du jalon 6 avec un nettoyage et une empreinte en cache : rien de perdu, réouverture sans rejouer de migration), en plus des tests v1 et v2 existants.
- **Volumes identifiés par leur GUID** (`\\?\Volume{…}\`, lu par `GetVolumeNameForVolumeMountPointW`), à défaut par numéro de série + système de fichiers (lecteurs sans GUID) — **jamais par leur lettre**, qui n'est qu'un dernier état connu (`volumes.letter`). Les racines et les chemins des snapshots sont relatifs au volume, sans lettre : une clé USB analysée en E: puis branchée en F: garde sa série ; un autre disque qui prend la lettre E: en a une autre (test `changement_de_lettre_de_volume`). Test Windows : le GUID de C: est lu, stable, unique parmi les volumes.
- **Snapshot agrégé par dossier** (`timeline/snapshot.rs`) : tailles allouées et nombres de fichiers des dossiers sur **4 niveaux au plus**, **jamais la liste des fichiers** ni un fichier seul ; dossiers de moins de 1 Mio non gardés (comptés dans leur parent), au plus 5 000 dossiers (les plus gros, donc toujours avec leurs parents). Chemins mis en commun entre snapshots (`snapshot_paths`).
- **Comparable seulement dans une même série** = même volume + même racine + même méthode (parcours ou MFT). Sinon la comparaison est refusée et l'interface dit pourquoi (« méthodes d'analyse différentes (parcours / MFT) », « pas le même dossier », « pas le même disque ») au lieu d'afficher un faux delta.
- **Rétention** (`timeline/retention.rs`), appliquée à chaque écriture, série par série : tous les snapshots des 7 derniers jours, puis un par jour jusqu'à 30 jours, un par semaine (du lundi) jusqu'à 6 mois, un par mois au-delà ; dans chaque case, le plus récent. Test sur **un historique synthétique d'un an** (4 snapshots par jour → environ 80) : rien de perdu sur 7 jours, au plus un par jour / semaine / mois ensuite, idempotente, et un mois plus tard aucun mois n'est vidé. Les chemins qui ne servent plus sont oubliés.
- **Taille de la base dans les Paramètres** (« Historique de la Timeline » : taille, snapshots, séries, règle de rétention) avec **« Purger… »** puis confirmation : supprime tous les snapshots et compacte la base (`VACUUM`) ; **le journal des nettoyages est gardé** (il sert à restaurer depuis la Corbeille).
- **Snapshots** : un à la fin de **chaque analyse** (sans bloquer l'affichage), et un **snapshot quotidien automatique seulement si l'appli tourne** — ni service ni tâche planifiée : l'interface le demande au backend une minute après le démarrage puis toutes les 30 minutes (`timeline_daily`) ; il reprend en arrière-plan les séries que l'utilisateur a analysées lui-même dans les 30 derniers jours et dont le dernier snapshot a plus de 20 h, sans toucher à l'analyse affichée ; une analyse lancée par l'utilisateur l'interrompt. **Jamais d'UAC** : une série analysée par la MFT n'a pas de snapshot quotidien sans droits d'administrateur, et l'écran le dit. Réglage « Snapshot quotidien » respecté.
- **Repères des nettoyages** sur la courbe : chaque nettoyage retient désormais son volume (`actions.volume_key`, ceux d'avant sont retrouvés par le numéro de série de leurs éléments) ; le repère montre ce qu'il a retiré **de la racine de la série** (éléments supprimés sous cette racine ; gouffres système : gain mesuré, seulement sur la série du disque entier) : pastille verte « −44,2 Go nettoyés », date et titre en infobulle.
- **Projection** (`timeline/projection.rs`) : régression linéaire (moindres carrés) **sur les 30 derniers jours, seulement avec au moins 5 points**, ce que les nettoyages de Lumen ont libéré étant rajouté aux points suivants (**les sauts dus aux nettoyages sont exclus**). Pente nulle ou négative (moins de 1 Mio par jour) : **pas de projection, un message neutre**. Sinon : « Au rythme des 30 derniers jours (9,9 Go par semaine, nettoyages de Lumen exclus), le disque sera plein dans environ 7 semaines. » (espace libre au dernier snapshot). Tests sur des séries connues (pente exacte, bruit symétrique, moins de 5 points, points hors fenêtre, plate, décroissante, saut de 44 Gio corrigé).
- **Diff entre deux dates** (`timeline/diff.rs`) : top des dossiers qui ont **le plus grossi** et **le plus rétréci**, **nouveaux** et **disparus** ; un parent dont un seul sous-dossier porte au moins 90 % de la variation s'efface devant lui ; un dossier nouveau ne cite pas ses sous-dossiers. **Un clic ouvre l'Explorer zoomé sur le dossier** (dans l'analyse affichée si c'est la même cible, sinon après l'analyse de la cible, dans l'historique précédent / suivant) ; un dossier disparu ou un disque absent n'est pas cliquable. Test sur deux snapshots de fixtures.
- **Écran Timeline** : choix de la série (« C: Système · dossier utilisateur · parcours »), période 7 j / 30 j / 3 mois / Tout, bandeau Utilisé / 7 derniers jours / Libre, courbe maison (SVG, graduations rondes, survol avec date, taille et origine du snapshot, points A et B de la comparaison), projection, état du snapshot quotidien, comparaison. Disque absent ou dont la lettre a changé : dit en une phrase. Le KPI « +18 Go / 7 j » de l'écran Analyse vient maintenant des snapshots (absent tant que l'historique est trop court).

### Décisions

- **Une seule base** pour le journal et les snapshots (le plan d'architecture le prévoyait) : les migrations restent linéaires et les repères des nettoyages se lisent par une simple requête.
- **Jours, semaines et mois de la rétention en UTC** : à quelques heures près à minuit, sans dépendre du fuseau.
- **Seuil des snapshots à 1 Mio** (et non 50 Mo comme dans le plan) avec un plafond de 5 000 dossiers : la comparaison voit les dossiers moyens ; « nouveau » peut aussi vouloir dire « passé au-dessus du seuil », ce que l'écran dit sous la comparaison. Taille typique : 50 à 150 Ko par snapshot.
- **Variation sur 7 jours** = dernier snapshot moins le dernier pris il y a au moins 7 jours (pas de valeur tant que l'historique est plus court).
- **Snapshot quotidien = une analyse complète en arrière-plan** (parcours, ou MFT seulement si l'appli est déjà élevée) : c'est le seul moyen d'avoir des tailles par dossier exactes ; elle cède la place à toute analyse de l'utilisateur.
- **CI : banc d'essai du scanner hors du chemin bloquant** (demande à la validation) : il prenait de 10 à 40 min selon le runner. Job `bench` à part, lancé seulement sur `main` après merge et à la main (`workflow_dispatch`), résultats dans le résumé du run ; les tests fonctionnels du scanner (job `scan`) restent bloquants sur chaque PR.
- Comparer avec une autre série reste possible dans la liste (« Autres séries (non comparables) ») pour que le refus soit visible et expliqué plutôt que caché.

### Écarts avec la maquette

- La maquette n'a pas d'écran Timeline détaillé : il reprend sa DA (conteneur rayon 16, bandeau construit comme les KPI, chiffres et chemins en mono, pastille verte des nettoyages, listes à bord fin). Captures : `design/captures/jalon-7/` (27 à 32 ; 01 à 26 refaites).

### Test manuel (jalon 7)

1. Analyse le dossier utilisateur : la Timeline montre une série « C: … · dossier utilisateur · parcours » avec 1 snapshot ; Paramètres › Historique de la Timeline affiche la taille de la base.
2. Crée un dossier de 2 Go dans Documents, relance l'analyse, puis Timeline › Comparer : le dossier est dans « Nouveaux » ; un clic ouvre l'Explorer zoomé dessus, et ← revient où tu étais.
3. Nettoie le cache npm (ou autre) depuis l'écran Analyse, relance l'analyse : un repère « −X Go nettoyés » apparaît sur la courbe, à la date du nettoyage.
4. « Scan rapide (admin) » du même dossier : une seconde série « … · MFT » apparaît ; la comparer avec un snapshot du parcours affiche le refus (« méthodes d'analyse différentes ») sans liste.
5. Clé USB : analyse-la, change sa lettre dans « Gestion des disques », analyse-la à nouveau : une seule série, avec la phrase « Ce disque était en E:… ». Débranche-la : la série reste consultable, « Disque absent ».
6. Laisse Lumen ouvert plus de 20 h (ou avance l'horloge de la VM) : un snapshot quotidien apparaît (« Dernier snapshot quotidien : … ») sans fenêtre UAC ; désactive « Snapshot quotidien » : plus rien ne se lance.
7. Avec moins de 5 snapshots sur 30 jours : « La projection demande au moins 5 snapshots… ». Avec un disque stable : message neutre, pas de date.
8. Paramètres › Purger… › Supprimer tout l'historique : la Timeline est vide, la taille de la base baisse, le Journal est intact.

---

## Jalon 6 — nettoyage avancé

Nouvelles formes de suppression (doublons, fichiers choisis) et premières commandes lancées en administrateur.

> **Non vérifié en réel** (au merge de la PR #4) : l'utilisateur a testé l'appli sur son PC **en lecture seule** (analyse, navigation, onglets affichés). Restent à vérifier :
> - **nettoyage réel** des dormants, doublons et gros fichiers — **non vérifié en réel**, à faire dans Windows Sandbox (points 1 à 6 ci-dessous) ;
> - **gouffres système** — **non vérifiés en réel**, à faire dans une VM Hyper-V avec checkpoint (points 7 à 12). D'ici là, l'onglet est **désactivé par défaut** derrière **Paramètres › Expérimental › Gouffres système** (réglage `experimentalSystem`, faux par défaut) : sans lui, l'onglet n'affiche qu'une explication et « Ouvrir les paramètres ».
>
> La CI Windows couvre ces chemins (fixtures `safety::win`, bout en bout sur faux profil), mais pas une vraie machine.
 **Onglet Gouffres système : à tester dans une VM Hyper-V avec un checkpoint pris avant** (Windows Sandbox ne gère ni les points de restauration ni l'hibernation, et DISM y fonctionne mal) ; les autres onglets en Windows Sandbox.

### Livré

- **Fichiers dormants** (`src-tauri/src/advanced/dormant.rs`) : fichiers non utilisés depuis 3 / 6 / 12 / 24 mois (6 par défaut, Paramètres), groupés par dossier.
  - **Date de dernier accès seulement si Windows la tient à jour** : `NtfsDisableLastAccessUpdate` lu dans le registre (la valeur qu'affiche `fsutil behavior query disablelastaccess`, bit 0 à 1 = désactivé, valeur absente = non fiable) **et** l'analyse l'a jugée fiable. Sinon **bascule sur la date de modification**, affichée en clair avec la valeur brute (« … = 0x80000003 : Lumen se base sur la date de dernière modification »).
  - Toujours **« À vérifier »**, rien de coché par défaut. Exclus : dossiers d'applis et système (`AppData`, `ProgramData`, `Program Files`, `Windows`, `node_modules`, jeux…, dossiers `.xxx` et `$…`), **projets git** (dossier contenant `.git` et tout ce qu'il contient), fichiers cachés ou système, fichiers cloud, denylist, éléments déjà couverts par une recommandation, fichiers de moins de 1 Mo.
- **Doublons** (`advanced/duplicates.rs`, lecture dans `safety::win::content`) — la seule fonctionnalité qui lit le contenu des fichiers :
  - lecture **avec la date de dernier accès gelée** (`SetFileTime` à `0xFFFFFFFF` sur le handle avant toute lecture ; si le gel est refusé, le fichier n'est pas lu), **E/S basse priorité** (`THREAD_MODE_BACKGROUND_BEGIN` pendant la lecture), **annulable** entre deux blocs de 1 Mio, progression par événements ;
  - **fichiers cloud jamais lus ni rapatriés** : attributs vérifiés avant l'ouverture, puis `FILE_FLAG_OPEN_NO_RECALL`, puis attributs revérifiés sur le handle ;
  - **les liens physiques ne sont pas des doublons** : un fichier (file ID) n'est compté qu'une fois, ses autres noms sont écartés et comptés à part ;
  - même taille → mêmes bords (64 Kio au début et à la fin, et la taille) → même contenu complet (BLAKE3) ;
  - **empreintes en cache** dans la base du journal (schéma v2, table `hash_cache`), par volume + file ID + taille + date de modification ; un fichier modifié est relu ; oubli après 6 mois sans servir ;
  - un **exemplaire à garder présélectionné** (le plus ancien, puis le chemin le plus court) ; **rien n'est coché par défaut**, « Cocher les copies » coche tout sauf lui ; l'interface empêche de cocher le dernier exemplaire d'un groupe ;
  - **`safety` refuse toute sélection qui retirerait tous les exemplaires d'un groupe** (`Refusal::LastCopy`) ou qui désignerait deux noms d'un même fichier (`Refusal::HardLink`) ; **au moment de la suppression**, il vérifie qu'un autre exemplaire non sélectionné est intact (identité, taille, date, pas un lien, même empreinte), puis **recalcule l'empreinte de l'élément** : si le contenu a changé, l'élément est sauté (« contenu modifié depuis la recherche de doublons »).
- **Gros fichiers** (`advanced/largest.rs`) : les **100 plus gros** de l'analyse, **filtre par type** (vidéos, images, audio, archives, images disque, installeurs, documents, autres) avec le nombre par type ; toujours **« À vérifier »** ; fichiers protégés, cachés, système et cloud exclus.
- **Gouffres système** (`safety::win::sinks` pour les faits lus sans droits, `safety::win::elevated` pour l'action) :
  - **commande officielle lancée directement en administrateur** par `ShellExecuteEx` (« runas ») : une demande UAC par commande, **binaire par son chemin complet** sous `%SystemRoot%\System32`, **arguments fixes issus de la liste fermée** (refus de tout argument contenant un espace, un guillemet ou un caractère de shell). **L'assistant MFT n'est pas élargi** : il reste mono-tâche. Code de sortie relevé ; refus de l'UAC signalé (« refusée (UAC) ») ;
  - **WinSxS** : `Dism.exe /Online /Cleanup-Image /StartComponentCleanup`, **sans `/ResetBase`** (non proposé : il rendrait les mises à jour installées impossibles à désinstaller), avec **`/LogPath:%LOCALAPPDATA%\Lumen\logs\dism-<horodatage>.log`** : le fichier est rattaché à l'entrée du journal (chemin et fin du fichier affichés dans le Journal), ce qui garde la sortie de DISM sans élargir les commandes ;
  - **Hibernation** : `powercfg /hibernate off` (libère `hiberfil.sys`, mais **plus de démarrage rapide** ni de veille prolongée, dit en clair) ou `/type reduced` (garde le démarrage rapide) ;
  - **Points de restauration** : uniquement le plus ancien, jamais tous, et **jamais le point que Lumen vient de créer ni le dernier restant**. La liste est d'abord lue en administrateur (bouton « Lire les points de restauration », lecture seule, une demande UAC) ; l'interface affiche **la date et la description du point qui serait supprimé**. Refusé **s'il existe moins de 2 points**, ou **si le plus ancien date de moins de 7 jours ou de cette session** de Lumen. Le plan désigne ce point précis (identifiant du cliché) ; au moment de l'action, un script figé (`-EncodedCommand`, seuls la lettre, l'identifiant et la date limite validés y sont insérés) **revérifie tout** — au moins 2 clichés, le plus ancien est toujours celui affiché, assez ancien — puis lance `vssadmin delete shadows /shadow={…} /quiet` sur ce seul cliché ; sinon rien n'est supprimé et le bilan dit pourquoi (codes 6, 7, 8). Le point créé juste avant par Lumen est le plus récent et date de la session : il ne peut jamais être celui-là ;
  - **Windows.old** : **`cleanmgr` uniquement**, lancé en interactif (`/d C:`) — coche « Installation(s) précédente(s) de Windows ». **Pas de `/sagerun`, donc aucune clé `StateFlags` écrite** dans le registre ;
  - **pagefile.sys** (et swapfile.sys) : **informatif**, sans action ;
  - **point de restauration proposé avant** (activé par défaut) : script PowerShell figé dans le binaire (`-EncodedCommand`, aucun texte variable), **prérequis obligatoire** : s'il n'est pas rempli, rien d'autre n'est lancé et le bilan dit pourquoi. **Protection du système désactivée sur le disque** (lue sans droits dans la liste des volumes protégés, `SPP\Clients`, et revérifiée par le script) : au lieu de bloquer sans issue, l'onglet le dit en une phrase (« Windows ne peut pas créer de point de restauration sur ce disque »), propose **« Ouvrir la protection du système »** et permet de lancer les actions **seulement après la case explicite « Continuer sans point de restauration »** — exigée par `safety` lui-même (`Confirmation::without_restore_point`, sinon rien n'est lancé) et **journalisée** (colonne `without_restore_point`, affichée dans le Journal). Même chemin si la création échoue au moment de l'action : le bilan propose les réglages ou « Continuer sans point de restauration… ». **Limite de Windows d'un point par 24 h** gérée : le script lit la fréquence (`SystemRestorePointCreationFrequency`, 1 440 min par défaut) et, si un point plus récent existe, **sort avec le code 3 que Lumen affiche** (« un point de restauration existe déjà depuis moins de 24 h : il sert de point de retour ») au lieu d'échouer en silence ; protection désactivée (5) ou point non créé (4) : arrêt avant les actions ;
  - **gain mesuré par l'espace libre du lecteur système avant et après**, journalisé comme au jalon 5 (type `system`, commandes, codes de sortie et leur signification).
- **Plan, exécution, journal communs** : `advanced_plan` (requête `dormant` / `largest` / `duplicates` avec des identifiants de nœuds **proposés par la dernière recherche**, ou `system` avec des actions de la liste fermée) → même `cleanup::prepare`, même confirmation (empreinte), même exécution, même journal. Le front n'envoie jamais de chemin. Journal en **schéma v2** (migration automatique) : type de nettoyage, raison d'un arrêt avant les actions, cache des empreintes.
- **Interface** : écran **Nettoyage** à 4 onglets (dormants, doublons, gros fichiers, gouffres système) ; confirmation adaptée aux commandes élevées (prérequis obligatoire, « autorisation administrateur (UAC) », « une fois lancée, une commande système se termine seule ») ; bilan avec « déjà fait » (point de restauration récent) et l'arrêt éventuel ; journal avec le type de nettoyage.
- **Tests** : `safety` 84 tests portables + 42 sous Windows, dont nouveaux : un seul point de restauration qui vient d'être créé jamais supprimé, point récent ou de la session gardé, identifiants et fichiers de sortie validés, sans point de restauration seulement après confirmation explicite, **fixture Windows** : lecture réelle des points de restauration puis suppression refusée par le script (moins de 2 points, ou pas le cliché affiché) sans qu'aucun point ne disparaisse ; doublons jamais tous les exemplaires, liens physiques jamais des doublons, doublon modifié avant suppression sauté, exemplaire gardé disparu → rien supprimé, commandes élevées seulement si le plan l'autorise, point de restauration obligatoire avant les actions ; **fixtures Windows** : hash sans toucher au dernier accès (volume de test où il est tenu à jour), hash du contenu et des bords, fichier cloud jamais lu, commande élevée et son code de sortie, script du point de restauration exécuté, gouffres lus sans admin. Appli : dormants (base de date, exclusions), gros fichiers, doublons (liens physiques, cloud, applis, cache, annulation), sélections → autorisations, actions système (jamais `/ResetBase` ni `/all`), journal v2 (migration, cache, bilans avec arrêt). Interface (vitest). **Bout en bout sur la CI Windows** : faux profil analysé, doublons trouvés par lecture réelle (lien physique, projet git et AppData écartés, dernier accès inchangé, cache), sélection de tous les exemplaires refusée, une copie supprimée (Corbeille) et l'autre **modifiée juste avant la suppression puis sautée**, dormant supprimé, le reste intact.

### Retours de test sur PC (avant merge)

- **Mise en page pleine largeur** : plus de largeur maximale ; marges latérales de 28 px ; la colonne Recommandations reste entre 360 et 440 px (`clamp(360px, 24vw, 440px)`) et la treemap prend le reste ; sous 1 000 px, la colonne passe sous la treemap. Vérifié en 1280, 1920 et 2560 px (captures 24).
- **Précédent / suivant dans l'Explorer** (`src/app/navigation.ts`) : boutons ← et → à gauche du fil d'Ariane, grisés quand il n'y a rien à faire, infobulle « Précédent : Local » / « Suivant : DXCache ». Historique des zooms (tuile, « Étendre », fil d'Ariane) et des changements de disque. Raccourcis déclarés dans `shortcuts.ts` : Alt+← / Alt+→, Retour arrière (hors saisie de texte), boutons 4 / 5 de la souris ; seulement sur l'écran Analyse, sans fenêtre ouverte par-dessus. Un emplacement est retenu **par son chemin** : après une nouvelle analyse ou un retour sur un autre disque (qu'il faut réanalyser, l'appli ne garde que la dernière analyse), le dossier est retrouvé par `scan_find`.
- **« Étendre »** (deux flèches opposées en diagonale ↗ ↙, trait fin comme ← → du fil d'Ariane, SVG maison) : curseur main, infobulle « Étendre : NomDuDossier » à la place de celle de la tuile ; au survol, en haut à droite de la tuile survolée la plus profonde qui a des enfants, à tous les niveaux ; un clic zoome sur ce dossier et l'ajoute à l'historique (au clavier : Entrée sur la tuile).
- **Profondeur adaptative** : le backend envoie enfants et petits-enfants, puis jusqu'à 4 niveaux sous les dossiers d'au moins 1 % de la vue (`Scan::slice_adaptive`, nombre de nœuds borné) ; le front montre les enfants d'une sous-tuile dès qu'elle fait au moins 120 × 80 px (`core/treemap/depth.ts`). Un voile de plus par niveau garde les bords visibles (contraste AA vérifié pour 2 et 3 voiles).
- **Shaders de jeux** (DXCache / GLCache NVIDIA, AMD, D3DSCache) : « À vérifier », catégorie Jeux, retirés des recommandations et des KPI de l'écran Analyse (champ `screen = "games"` des règles : aucun gain ni lien sur l'écran Analyse), toujours visibles dans la treemap en couleur Jeux, sans hachures. Explication pour le jalon 8 : « Les jeux recompileront leurs shaders : premiers lancements plus longs et saccades temporaires. », et un conseil (`tip`) : la taille maximale du cache se règle dans le panneau NVIDIA / AMD Software. `sans-risque.txt` régénéré. Le `shadercache` de Steam est sous Program Files (denylist) : il reste classé Jeux par la zone `jeux`, sans règle.
- **Nœud « N autres éléments »** : statut et catégorie majoritaires (en octets) de ce qu'il regroupe, gains additionnés, recommandation commune s'il n'y en a qu'une (`Applied::aggregate`) — plus de « À garder » sur un regroupement de fichiers sans risque.
- **Libellés rognés** : en-têtes de tuile sur une seule ligne (nom tronqué, gain à droite), taille en bas affichée seulement si elle tient en entier ; dans une sous-tuile étroite, la taille est omise plutôt que coupée.

### Note pour plus tard

- Revenir à un autre disque par ← relance une analyse complète (l'appli ne garde que la dernière). À faire : **garder en mémoire la dernière analyse de chaque disque visité pendant la session**, pour que ← / → soient instantanés (mémoire bornée, analyses les plus anciennes oubliées d'abord).

### Décisions

- **Gouffres système par `ShellExecuteEx` « runas » plutôt que par l'assistant élevé** (demande à la validation du jalon 5) : l'assistant MFT reste muet, éphémère et mono-tâche. Conséquences assumées : **un processus élevé ne peut pas être arrêté** par Lumen non élevé (annuler ou le délai maximal ne font que cesser d'attendre, ce que le bilan dit), et **ses sorties ne sont pas capturables** : le journal garde le code de sortie et sa signification ; le gain est mesuré sur le disque.
- **cleanmgr en interactif plutôt que `/sagerun`** : pas de clé `StateFlags` à écrire puis retirer dans `HKLM\…\VolumeCaches`, l'utilisateur voit et coche lui-même « Installation(s) précédente(s) de Windows ».
- **Point de restauration par un script PowerShell figé** (Checkpoint-Computer) passé en `-EncodedCommand` : aucun texte variable, binaire par son chemin complet, mêmes règles que les autres commandes élevées. Code 3 = un point récent existe : **compte comme rempli**.
- **Plus ancien point de restauration** (correction avant merge) : un point de moins de 7 jours n'est jamais supprimé, ni un point créé pendant la session de Lumen (date limite = la plus ancienne des deux). Le cliché supprimé est celui qui a été affiché, désigné par son identifiant et retrouvé par le script : `/oldest` à l'aveugle n'est plus utilisé. Lire la liste demande une autorisation administrateur à part (lecture seule) : sans elle, l'action reste grisée.
- **DISM** : `/LogPath` vers `%LOCALAPPDATA%\Lumen\logs` (créé par Lumen) ; c'est le seul argument variable d'une commande élevée, mis entre guillemets parce qu'il peut contenir des espaces, et refusé s'il contient un guillemet, un caractère de contrôle ou de shell. Le journal SQLite passe en **schéma v3** (chemin du fichier rattaché à la commande, « sans point de restauration » sur l'action), migration testée depuis v1 et v2.
- **Empreinte = BLAKE3(taille ‖ contenu)** ; les bords (64 Kio + 64 Kio) ne servent qu'à écarter vite les fichiers différents, jamais à décider d'un doublon.
- **Doublons : seuil 1 Mo par défaut** (Paramètres), au plus 500 groupes affichés (les totaux restent exacts). Dormants : seuil 1 Mo.
- **Rien n'est coché par défaut** dans les quatre onglets : tout y est « À vérifier ». Pas de hachures (réservées au « sans risque »).
- Les sélections ne valent que pour la dernière recherche de l'analyse affichée : un identifiant qui n'en vient pas est refusé.

### Écarts avec la maquette

- La maquette n'a pas d'écran Nettoyage : il prolonge sa DA (même conteneur et même en-tête que l'Explorer, contrôles segmentés, listes à bord fin, chemins et tailles en mono, pastille « À vérifier » ambre, bouton vert « Nettoyer »). Captures : `design/captures/jalon-6/` (17 à 23).

### Test manuel (jalon 6) — Windows Sandbox (1 à 6), VM Hyper-V (7 à 12)

1. Dans la Sandbox : `fsutil behavior query disablelastaccess`, puis analyse le profil et ouvre **Nettoyage › Fichiers dormants** : la phrase sous le choix des mois cite la même valeur et dit sur quelle date Lumen se base. Change-la (`fsutil behavior set disablelastaccess 1`, redémarrage de Lumen) : la bascule sur la date de modification est annoncée.
2. Crée un vieux fichier (`(Get-Item x.zip).LastWriteTime = (Get-Date).AddYears(-1)`) dans Documents, un autre dans un dossier contenant `.git` et un dans `AppData` : seul le premier est proposé, rien n'est coché.
3. **Doublons** : copie une vidéo à deux endroits, crée un lien physique (`New-Item -ItemType HardLink`) vers l'original. « Rechercher les doublons » : un groupe de 3, le lien n'y est pas ; la date de dernier accès des fichiers est inchangée (`(Get-Item f).LastAccessTime` avant / après).
4. Essaie de cocher tous les exemplaires d'un groupe : le dernier ne se coche pas. « Cocher les copies », puis modifie une copie avant de confirmer : elle est sautée (« contenu modifié »), l'autre part à la Corbeille.
5. Relance la recherche : elle est quasi immédiate (empreintes en cache).
6. **Gros fichiers** : le filtre par type fonctionne, rien n'est coché ; nettoie un fichier, il est dans la Corbeille et au journal.
Points 7 à 12 : **VM Hyper-V** (Windows 11 de développement), **checkpoint Hyper-V pris avant**.

7. **Gouffres système** : coche WinSxS et « Réduire » l'hibernation, garde le point de restauration : la confirmation annonce une demande UAC par commande. Confirme : UAC pour le point de restauration, puis pour chaque commande ; le bilan montre le gain mesuré sur C: et « Journal complet : …\dism-<horodatage>.log » ; le Journal affiche la fin de ce fichier.
8. Relance aussitôt : le point de restauration est signalé « déjà fait » (moins de 24 h), les actions continuent.
9. **Un seul point de restauration** (celui que Lumen vient de créer) : « Lire les points de restauration » → 1 point, « il en faut au moins 2 », la case reste grisée. Avec un point de plus de 7 jours en plus (avancer l'horloge de la VM, ou un ancien checkpoint) : la date et la description du point qui serait supprimé s'affichent ; après l'action, l'autre point est toujours là (`vssadmin list shadows`).
10. Désactive la protection du système sur C: : l'onglet le dit en une phrase, « Ouvrir la protection du système » ouvre le bon réglage ; « Préparer les actions » demande de cocher « Continuer sans point de restauration » ; le Journal indique « Lancé sans point de restauration ».
11. Refuse l'UAC d'une commande : elle est « refusée (UAC) », rien d'autre ne casse.
12. Windows.old (si présent) : le Nettoyage de disque s'ouvre ; aucune clé `StateFlags` n'apparaît sous `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches`.

---

## Jalon 5 — scripts, exécution, journal

Première vraie suppression. **À tester d'abord dans Windows Sandbox ou une VM**, pas sur le PC principal.

### Livré

- **Exécution réelle dans `safety`** (`exec.rs`, logique pure ; effets réels dans `safety::win`) : `plan::execute` est le seul point d'entrée, après vérification de l'empreinte du plan affiché (et de la seconde confirmation pour le définitif). Le verrou de compilation du jalon 2 est retiré.
  - **Recontrôle complet au moment de l'action** : chaque élément du plan repasse les 6 étapes ; un dossier est **relu élément par élément** (handle ouvert sur le dossier lui-même, identité vérifiée) et chaque élément qu'il contient repasse liste blanche, denylist (secrets compris), liens, fichiers cloud, identité / taille / date, âge minimum de la règle et verrou, contre ce que le scan avait vu (le plan porte le contenu attendu de chaque dossier, couvert par l'empreinte BLAKE3).
  - **Tout ce qui a changé est sauté et signalé, jamais supprimé** : élément apparu, modifié, remplacé, devenu lien, verrouillé, protégé. Un dossier qui a changé n'est jamais traité d'un bloc : seuls ses sous-ensembles intacts partent.
  - **Corbeille** par `IFileOperation`, un élément par opération (bilan exact), pré-contrôle du volume refait juste avant, `FOF_WANTNUKEWARNING` ; un « sink » note l'emplacement de l'élément dans la Corbeille (`$R…`) pour la restauration.
  - **Définitif** (option) : du plus profond au plus haut, chaque élément est ouvert sans suivre les liens, son identité est **relue sur le handle ouvert** puis il est marqué pour suppression sur ce même handle (`FileDispositionInfoEx`, POSIX). Un élément substitué entre le contrôle et l'action (jonction, autre fichier de même nom et taille) n'est jamais supprimé ; un dossier qui a reçu un nouvel élément reste.
  - **Progression** élément par élément, **annulable entre deux éléments** (et pendant le recontrôle d'un gros dossier), **échecs partiels** gérés : on continue, puis bilan libéré / sauté / refusé / échoué / annulé avec la raison. **Gain réel mesuré** par l'espace libre des volumes avant et après, en plus de la somme estimée.
- **Commandes officielles** : binaire **résolu par son chemin complet** (emplacements d'installation connus, puis entrées absolues du PATH ; fichier ordinaire, ni lien ni cloud ; jamais le dossier courant ni un dossier temporaire) ; npm lancé par `node.exe` + `npm-cli.js` (pas de `npm.cmd`), outils système depuis `System32` seulement. **Prérequis d'abord** (`gradle --stop` avant les caches Gradle), **délai maximal** par commande, sans console ni shell, dans un **objet job** (tout l'arbre de processus est arrêté au délai ou à l'annulation), **stdout/stderr capturés** dans le journal. Une commande qui demande l'administrateur n'est jamais lancée par Lumen : elle est signalée et reste dans le script.
- **Script copié** (`script.rs`), qui s'exécute hors de `safety` et embarque ses propres garde-fous : date de génération dans l'en-tête et **refus au-delà de 24 h** ; pour chaque chemin `Test-Path`, **refus si l'élément ou un parent est un lien** (attribut ReparsePoint) ou un fichier cloud, **refus si la taille ou la date diffère de la génération** (dossier : nombre de fichiers et de dossiers, taille totale, date la plus récente, mesurés sans jamais entrer dans un lien ; un dossier qui contient un lien est refusé), refus hors d'un disque local fixe ; Corbeille via `Microsoft.VisualBasic.FileIO` (`SendToRecycleBin`), **jamais `Remove-Item -Recurse`** ; **simulation par défaut** (`-WhatIf` actif tant que `-Executer` n'est pas donné), `$ErrorActionPreference = 'Stop'` élément par élément, **journal dans un fichier**, bilan et code de sortie. Commandes écrites avec le chemin complet résolu par Lumen, revérifié avant lancement.
- **Journal SQLite** (`%LOCALAPPDATA%\Lumen\lumen.db`, WAL, schéma versionné) : chaque nettoyage (méthode, empreinte, bilan, prévu / libéré / mesuré, **script équivalent**), chaque élément (méthode, résultat, taille, raison, file ID, emplacement dans la Corbeille), chaque commande (chemin complet, arguments, statut, code de sortie, sorties). **« Restaurer depuis la Corbeille »** tant que c'est possible : seulement un `$R…` de la Corbeille de l'utilisateur dont le `$I…` désigne le chemin d'origine du journal et dont le file ID est celui de l'élément supprimé ; renommage vers l'emplacement d'origine **sans jamais écraser**, parents sans lien.
- **Appli** : `plan_create` (rien n'est fait), `plan_script`, `plan_execute` (empreinte du plan affiché), `plan_cancel`, `journal_list` / `journal_detail` / `journal_script` / `journal_restore`. Les autorisations viennent des règles elles-mêmes (cible développée, portée, âge minimum ; artefacts de projet entiers), le contenu attendu des dossiers de l'analyse.
- **Interface** : fenêtre de **confirmation** (total, nombre d'éléments, **liste dépliable**, méthode — Corbeille / commande officielle / les deux —, disque et espace libre, éléments refusés par `safety`) ; le bouton porte le total (« Mettre 43,4 Go à la Corbeille ») ; **définitif** seulement si autorisé dans Paramètres, en **seconde confirmation où il faut taper le total ou « SUPPRIMER »** ; progression avec annulation ; **bilan** (libéré, mesuré sur le disque, faits / sautés / refusés / échoués avec la raison, statut des commandes), puis « Voir le journal » ou « Relancer l'analyse ». **Écran Journal** : chaque nettoyage, ses éléments, ses commandes et leurs sorties, restauration, copie du script exécuté. « Copier le script » copie désormais le script complet de `safety`.
- **Tests** : `safety` 73 tests portables + 37 Windows (dont fixtures réelles : jonction substituée après le plan, identité relue sur le handle, lecture seule, liste par handle de 1 500 noms, délai dépassé qui arrête petit-enfant compris, Corbeille puis restauration sans écraser, commande npm résolue par son chemin complet) ; **Pester** sur le script généré (simulation par défaut, Corbeille, fichier modifié après génération, jonction piégée sur l'élément / un parent / dans le contenu, script expiré, définitif) ; journal SQLite ; sélection → cibles / autorisations / commandes ; parcours complet de l'interface (vitest) ; **bout en bout sur la CI Windows** : faux profil analysé, nettoyage réel des recommandations sans risque, puis vérification de ce qui est parti (Corbeille) et de ce qui est intact (`%TEMP%` récent, projet sans verrou, documents, `~\.ssh`, Téléchargements), journal complet, restauration.

### Décisions

- **Définitif par handle plutôt que par `IFileOperation`** : identité relue sur le handle ouvert, jamais à travers un lien ; plus strict que le Shell, qui travaille par chemin.
- **Un élément par opération de Corbeille** : un échec n'interrompt pas les autres et le bilan dit exactement ce qui est parti.
- **Commandes « administrateur » jamais lancées par Lumen au jalon 5** (Nettoyage de disque, DISM…) : signalées, laissées au script (console élevée) ; les actions système passeront par l'assistant élevé avec leurs propres garde-fous (jalon 6).
- **npm sur son cache par défaut** : `npm cache clean --force` vide le cache configuré de npm ; le gain réel est mesuré par l'espace libre, même s'il diffère du dossier analysé.
- **Script : `-Executer` pour agir.** La simulation est le comportement par défaut, sans rien à retenir ; `-WhatIf` reste accepté. Le script ne connaît pas le quota de la Corbeille de chaque volume : il ne traite que des éléments que `safety` a déjà pré-contrôlés (disque fixe, Corbeille active, taille).
- **Un seul plan confirmable à la fois** (le dernier affiché) ; un plan exécuté ne peut pas l'être une seconde fois. Après un nettoyage, l'analyse affichée est ancienne : « Relancer l'analyse » est proposé au bilan.
- Le journal texte mensuel prévu au plan (`logs\lumen-AAAA-MM.log`) viendra avec l'écran Logs (jalon 11) ; le journal SQLite et le journal de chaque script couvrent le besoin d'ici là.

### Écarts avec la maquette

- La maquette ne montre ni la confirmation ni le journal : les deux prolongent sa DA (surface des popovers, chiffres et chemins en mono, pastilles de statut, hachures vertes pour le « sans risque », bouton vert « Nettoyer »). Captures : `design/captures/jalon-5/` (12 à 16).

### Test manuel (jalon 5) — dans Windows Sandbox ou une VM

1. Installe Lumen dans la Sandbox, crée un faux projet (`package.json`, `package-lock.json`, `npm install`), quelques fichiers dans `%TEMP%` (dont un vieux : `(Get-Item x.tmp).LastWriteTime = (Get-Date).AddDays(-3)`), analyse le profil.
2. « Nettoyer » : vérifie le total, le nombre d'éléments, la liste dépliée, la méthode et le disque ; le bouton affiche le total.
3. Confirme : progression, puis bilan (libéré, mesuré sur le disque). Ouvre la Corbeille : les éléments y sont.
4. Vérifie que les fichiers récents de `%TEMP%`, tes sources et le `package.json` sont intacts.
5. Journal : l'action y est avec ses éléments et la sortie de `npm cache clean` ; « Restaurer depuis la Corbeille » remet `node_modules` à sa place.
6. Recommence en modifiant un fichier de `node_modules` entre « Nettoyer » et la confirmation : il est laissé et signalé (« modifié depuis l'analyse »), le reste part.
7. Ouvre un fichier de `%TEMP%` dans un éditeur pendant le nettoyage : il est sauté (« ouvert par un autre programme »).
8. Paramètres › Autoriser la suppression définitive, puis « Définitif » dans la confirmation : la seconde étape exige de taper le total ou SUPPRIMER.
9. « Copier le script », colle-le dans PowerShell : simulation par défaut (rien ne bouge), puis `-Executer` ; essaie-le après avoir modifié un fichier : refusé.
10. Annule un gros nettoyage en cours : il s'arrête après l'élément en cours, le bilan l'indique.

---

## Jalon 4 — moteur de règles

### Livré

- Crate `src-tauri/rules/` (logique pure, sans React ni Tauri) :
  - **`rules.toml`** embarqué, **validé au chargement** (`schema.rs`) : champs inconnus refusés, identifiants uniques, textes français (référence) et anglais obligatoires, commandes de la liste fermée de `safety` seulement, cohérence portée / âge / action. **Aucun chemin absolu en dur** : une cible commence toujours par une variable connue — `%USERPROFILE%`, `%LOCALAPPDATA%`, `%APPDATA%`, `%TEMP%` pour les règles ; `%SYSTEMDRIVE%` pour une commande officielle seulement (Windows.old) ; les zones de classement ont en plus `%DOCUMENTS%`, `%WINDIR%`, `%PROGRAMFILES%`… Pas de `..`, pas de `.`, pas de lecteur ni de partage, `**` jamais en fin de cible. Une seule règle invalide : aucune règle n'est appliquée.
  - Valeurs des variables : dossiers connus de Windows (`safety::win::rule_variables`, chemins longs), jamais l'environnement du processus (sauf `%TEMP%`, puis vérifié).
  - **Moteur** (`engine.rs`) : un seul parcours de l'arène, un automate par motif (`*`, `?`, `**`, insensible à la casse). Portées : la cible entière, son contenu, ou son contenu inchangé depuis N heures (sous-arbre entier). Un élément n'est couvert que par une règle (la première), une correspondance plus profonde dans un élément déjà couvert est ignorée, les gains remontent jusqu'à la racine séparés par statut.
  - **Jamais la denylist** : chaque élément couvert par une règle de suppression, et tout ce qu'il contient, passe `DenyList::check` et les vérifications de faits de `safety` (attribut SYSTEM, dossier contenant `.git` — ni entier, ni en partie). Sinon seuls ses enfants sûrs sont pris. Une cible littérale qui tombe dans la denylist sur la machine (`%TEMP%` = `C:\Windows\Temp` d'un compte de service) est neutralisée et journalisée. Les règles « commande » (Windows.old par le Nettoyage de disque) mesurent seulement : rien n'est couvert pour suppression.
  - **Projets** : artefacts (`node_modules`, `.next`, `target`, `build`, `obj`…) comptés seulement si un marqueur du projet est à côté (`package.json`, `Cargo.toml`, `build.gradle`, `*.csproj`…) ; `node_modules` « sans risque » exige en plus un fichier de verrouillage (régénération à l'identique) ; `dist` et `bin` exigent d'être exclus par le `.gitignore` du projet ; jamais sous `AppData`, `%LOCALAPPDATA%`, `%APPDATA%`, les dossiers `~\.*` (extensions d'éditeur…) ni `~\scoop` (applis installées qui embarquent un `node_modules`). Actif / inactif d'après la **date du dernier commit lue dans `.git` sans lancer git** (`git.rs` : `HEAD` et branches locales, références isolées ou `packed-refs`, objets isolés zlib ou packfile index v2 avec deltas `OFS_DELTA` / `REF_DELTA`, SHA-1 et SHA-256, worktrees et sous-modules via `.git` fichier et `commondir`) ; sans git, fichier source modifié le plus récemment hors artefacts et `.git`. Inactif = 180 jours.
  - Lecture de `.git` et `.gitignore` sans changer leur date de dernier accès (`fsread.rs` : handle à date gelée, comme le scan).
- **Règles livrées** (`rules/rules.toml`) — sans risque : caches npm, Yarn, pnpm (store prune, gain « jusqu'à »), NuGet, Gradle (après `gradle --stop`), pip, uv, caches de shaders DirectX / NVIDIA / AMD, artefacts des projets actifs, fichiers temporaires de plus de 48 h ; à vérifier : registre Cargo, dépôt Maven, Android SDK (images système, plateformes), émulateurs Android, modèles Ollama et Hugging Face, artefacts des projets inactifs, rapports de plantage, Windows.old (Nettoyage de disque). Classement : système, jeux (Steam, Epic, GOG, Ubisoft, EA), documents et médias (dossiers connus), projets (marqueurs), caches de paquets, Android, modèles IA ; par défaut « Outils dev & autres ».
- **Liste à relire** : `src-tauri/rules/sans-risque.txt`, générée par le test `liste_des_regles_sans_risque_a_relire` (cibles, portée, commandes, artefacts et conditions, « régénéré par »). Toute règle « Sans risque » ajoutée ou modifiée fait échouer le test tant que la liste n'est pas régénérée (`LUMEN_UPDATE_RULE_LISTS=1 cargo test -p rules`) : le changement apparaît donc dans la PR.
- **Appli** : règles appliquées à la fin de chaque analyse (phase « application des règles ») ; commandes `recommendations`, `recommendation_detail`, `rule_kpis` ; chaque nœud de la treemap reçoit son statut, sa catégorie, sa recommandation et son gain. Plus aucune catégorie devinée par le nom (`core/categories.ts` supprimé) : la carte en direct de la première analyse est neutre.
- **Interface** : KPI réels (libre après nettoyage = libre + sans risque, sans risque, à vérifier) ; recommandations groupées par statut ; **seuls les « Sans risque » sont cochés par défaut** ; panneau de détail avec explication, fichiers concernés, nombre de fichiers / dossiers / projets, dernière utilisation ou activité, et aperçu des commandes officielles. Survol d'une recommandation : les éléments couverts et leurs parents restent nets, quel que soit le zoom.
- **Tests** : 21 tests du crate `rules`, dont schéma strict et chemins interdits (15 formes refusées), motifs, git (objet isolé et `packed-refs`, packfile avec deltas en SHA-1 et SHA-256, worktree, dépôt vide, objet manquant, référence qui sort du dépôt, deltas invalides) ; **aucune cible dans la denylist** sur trois dispositions de profil ; **aucun élément couvert dans la denylist** sur un volume `C:` complet piégé (ruche, fichier SYSTEM et dépôt git dans `%TEMP%`, profil d'un autre utilisateur, Windows.old, appli Electron dans AppData, extension d'éditeur) ; `%TEMP%` dans `C:\Windows` neutralisé ; Lumen sous le compte SYSTEM : rien de couvert ; projets actifs / inactifs et garde-fous ; statuts, catégories, gains. Windows (CI) : variables des règles = dossiers connus (`safety`), dépôts **réels créés par git** (objets isolés, après `git gc`, SHA-256, worktree), lecture de `.git` sans toucher au dernier accès.

### Compléments demandés à la validation

- **Secrets et configurations protégés par `safety` lui-même** (nouveau motif `DenyReason::Secret`), donc aussi au moment de l'exécution réelle (jalon 5) : `.npmrc`, `.yarnrc*`, `.git-credentials`, `.netrc`, clés SSH, magasins de clés (`*.jks`, `*.keystore`, `*.pfx`, `*.p12`), coffres KeePass, clés PuTTY, stockage des navigateurs et applis Electron (`Cookies`, `Login Data`, `Web Data`, `Local Storage`, `Session Storage`, `IndexedDB`, `Sessions`, `logins.json`, `key4.db`…) **où qu'ils soient, même dans un dossier de cache** ; et dans le profil : `~\.ssh`, `~\.aws`, `~\.azure`, `~\.kube`, `~\.gnupg`, `~\.gradle\gradle.properties`, `~\.gradle\init.d`, `~\.m2\settings.xml`, `~\.m2\settings-security.xml`, `~\.cargo\credentials*`, `~\.cargo\config*`, `~\.docker\config.json`, `~\.android\adbkey*`, `debug.keystore`. Les dossiers d'outil qui les contiennent (`~\.gradle`, `~\.m2`, `~\.cargo`, `~\.docker`, `~\.android`) sont refusés entiers ; leurs sous-dossiers de cache restent autorisés. Test `safety` `secrets_et_configurations_jamais_supprimes`.
- **Tests des règles** : `aucune_regle_ne_couvre_un_secret` (chaque secret de la liste à sa place, et les mêmes noms glissés dans chaque dossier visé par une règle : `%TEMP%`, caches de shaders et de Gradle, registre Cargo, `node_modules`, `target`) ; `cibles_jamais_a_la_racine_d_un_dossier_d_outil` ; `rien_dans_les_telechargements` (Téléchargements ajouté aux exclusions des projets). `%TEMP%` : contenu inchangé depuis 48 h seulement (vérifié par le test). Les règles « commande » (npm, Yarn, pnpm, NuGet, pip, uv) ne suppriment rien elles-mêmes : c'est l'outil qui vide son propre cache.

### Décisions

- **Actifs « sans risque », inactifs « à vérifier »** (comme la maquette validée) : les artefacts d'un projet actif seront reconstruits au prochain build de toute façon ; ceux d'un projet abandonné peuvent dépendre de versions qui ne s'installent plus.
- **Commandes officielles : mesure seulement.** Une règle `action = "command"` (npm, Yarn, pnpm, NuGet, pip, uv, Nettoyage de disque) n'autorise aucune suppression directe ; la taille est celle des dossiers que la commande vide (« jusqu'à » pour `pnpm store prune`). Gradle reste une suppression, précédée de `gradle --stop`.
- **pip et uv séparés** (deux règles) : chacun a sa propre commande, et l'un peut être absent.
- **Docker / WSL (compactage)** reporté au jalon 8 (écran Dev) : le gain dépend de l'espace libre à l'intérieur du disque virtuel, qu'il faut mesurer avec WSL arrêté ; l'annoncer sans mesure serait trompeur.
- **Détail : aperçu des commandes seulement** au jalon 4 ; le script complet de `safety` est copié depuis le jalon 5.
- La note de la maquette « Suppression définitive » (données factices) est corrigée : la Corbeille est la règle par défaut.

### Test manuel (jalon 4)

1. Analyse de ton profil : la colonne de droite liste des recommandations réelles, « Sans risque » d'abord ; seules celles-là sont cochées.
2. KPI : « Libre après nettoyage » = espace libre + total sans risque ; « Sans risque » et « À vérifier » donnent le nombre d'éléments.
3. Mode « Nettoyage » de la treemap : vert (hachuré) pour le sans risque, ambre pour l'à vérifier, gris pour le reste ; mode « Catégorie » : Documents en violet, projets en rose, caches en vert d'eau…
4. Survole une recommandation : seules ses tuiles restent nettes, au premier niveau comme après un zoom.
5. Ouvre le détail d'un cache npm, d'un projet actif et de « Artefacts de projets inactifs » : fichiers concernés, nombre de projets, dernière activité (date de ton dernier commit).
6. Vérifie qu'un `node_modules` sans `package-lock.json` (ou yarn/pnpm) n'est pas en « Sans risque », et que rien sous `AppData\Local\Programs` (VS Code, Discord…) ni `~\.vscode` n'apparaît.
7. Relis `src-tauri/rules/sans-risque.txt` (dans la PR) : c'est la liste exhaustive de ce que Lumen proposera de nettoyer sans te demander de vérifier.

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

### Mesures (CI Windows, run 39456100757)

Runner GitHub `windows-latest` (Windows Server 2025, 4 vCPU, disque géré), administrateur.

| mesure | volume | taille allouée | durée | pic mémoire |
|---|---|---|---|---|
| génération | 200 000 fichiers, 404 dossiers | | 17,7 s | |
| parcours | 200 000 fichiers | 1,97 Gio | 1,02 s | 31 Mio |
| MFT | 200 000 fichiers | 1,97 Gio | 6,63 s | 45 Mio |
| parcours de `C:\` (cache froid) | 1 139 609 fichiers, 208 873 dossiers | 119,12 Gio | 481,8 s | **197 Mio** (133 Mio après) |
| parcours de `C:\` (cache chaud) | idem | 119,14 Gio | 11,3 s | |
| MFT de `C:\` | 1 139 648 fichiers, 208 880 dossiers | 119,31 Gio | 7,1 s | **327 Mio** (136 Mio après) |

- **Mémoire** : le résultat d'une analyse de `C:\` (1,41 million d'entrées) occupe ~135 Mio ; le pic du parcours (197 Mio) vient des listes de dossiers en cours, celui de la MFT (327 Mio) de la table des enregistrements avant construction de l'arbre. Mesure par `GetProcessMemoryInfo` (pic de l'ensemble de travail).
- **Parcours = MFT sur `C:\`** (comparaison fichier par fichier, `bench compare`) : 0 fichier vu seulement par le parcours ; 24 fichiers (0,17 Gio) vus seulement par la MFT, tous dans les 18 dossiers que Windows refuse à un administrateur sans privilège de sauvegarde (`System Volume Information`, Defender ATP, `LogFiles\WMI\RtBackup`…) ; 43 fichiers de tailles différentes pour un écart total < 0,01 Gio (téléchargements Windows Update en cours d'écriture, quelques petits fichiers à la limite résident / non résident).
- **Écart corrigé pendant le jalon** : la première version prenait les tailles dans la liste du dossier. Sur `C:\`, 120 193 fichiers y avaient une taille périmée (autres noms des liens physiques de WinSxS, fichiers d'installeurs) : 108,0 Gio au lieu de 119,4. L'Explorateur Windows lit la même copie et affiche les mêmes valeurs fausses. Désormais chaque fichier est ouvert par son file ID pour ses attributs seulement ; test Windows `liste_de_dossier_perimee_corrigee`.
- **Durée** : le parcours à froid est limité par les lectures aléatoires du disque du runner (même code : 11 s à chaud) ; la MFT se lit séquentiellement. Sur un SSD NVMe, le parcours à froid devrait être proche du chiffre à chaud — à mesurer sur ta machine (étape 8 du test manuel). `GetCompressedFileSizeW` : 15 appels seulement sur `C:\`.

### Compléments demandés à la validation

- **Assistant administrateur muet et éphémère** (`scan/src/win/helper.rs`, `trust.rs`) :
  - il ne fait qu'une chose : lire la MFT d'un volume désigné par sa **seule lettre** (disque fixe NTFS, ni amovible, ni réseau, ni SUBST, vérifié des deux côtés), envoyer le résultat et quitter. Il ne reçoit plus aucun chemin : le dossier demandé (le profil, par exemple) est extrait ensuite par Lumen, sans privilège (`Scan::subtree`, réagrégé) ;
  - il n'écoute rien : c'est Lumen (non élevé) qui crée le tube — nom aléatoire (`BCryptGenRandom`), `FILE_FLAG_FIRST_PIPE_INSTANCE`, une seule instance, clients distants refusés, DACL réservée au SID de l'utilisateur courant. L'assistant s'y connecte en client avec `SECURITY_IDENTIFICATION` (le serveur du tube ne peut pas agir en son nom) ;
  - PID vérifié des deux côtés : Lumen exige que le client soit le processus rendu par `ShellExecuteEx` (`SEE_MASK_NOCLOSEPROCESS`, `GetNamedPipeClientProcessId`) ; l'assistant exige que le serveur soit le PID de Lumen reçu au lancement **et** le même exécutable (`GetNamedPipeServerProcessId`, `QueryFullProcessImageNameW`) ;
  - jetons aléatoires à usage unique (32 octets dans chaque sens, comparés en temps constant) : l'assistant ne lit sur le tube que ces 32 octets ; tout autre contenu est refusé et il quitte sans rien analyser. Il n'existe aucune commande ;
  - délais : l'assistant quitte seul si l'échange n'est pas fait sous 10 s (et Lumen abandonne de même), s'il perd Lumen (processus parent surveillé), et dans tous les cas après le scan ; au moindre écart, Lumen le termine ;
  - exécutable : avant tout lancement, Lumen vérifie que son exécutable est installé sous `Program Files` (chemin réel) et que ni lui, ni son dossier (pose d'une DLL), ni aucun dossier parent ne sont modifiables par l'utilisateur sans ses droits administrateur (`AccessCheck` avec le groupe Administrateurs désactivé). L'installeur passe donc en **installation pour tous les utilisateurs** (`perMachine`, Program Files, une demande UAC à l'installation). Un Lumen lancé depuis un autre dossier (`npm run tauri dev`, copie portable) ne propose pas le « Scan rapide (admin) » ;
  - tests Windows (`win/helper.rs`, `win/trust.rs`), dont de vrais processus assistants : client étranger connecté avant l'assistant → refusé ; mauvais jeton → refusé ; « commande » envoyée à la place du jeton → l'assistant quitte (code 6) sans rien envoyer ; lettre invalide, lecteur absent, lecteur SUBST ou chemin à la place de la lettre → refusés ; assistant sans réponse (tube inexistant ou muet) → arrêt seul en 10 s ; bout en bout en administrateur → résultat reçu, assistant terminé avec succès ; exécutable hors Program Files ou dans un dossier de Program Files ouvert à tous → refusé.
- **Dernier accès intact** : les fichiers ne sont ouverts que pour leurs attributs (`FILE_READ_ATTRIBUTES`, jamais `FILE_READ_DATA`) ; lister un dossier mettant à jour sa propre date, chaque dossier est listé par un handle où `SetFileTime(…, 0xFFFFFFFF, …)` gèle cette date (repli sans gel si `FILE_WRITE_ATTRIBUTES` est refusé, dossiers système). Les racines sont résolues par leur handle (`GetFinalPathNameByHandleW`) et non plus par `GetLongPathNameW`, qui liste les dossiers parents. Test Windows `scan_sans_toucher_au_dernier_acces` sur un volume NTFS neuf monté par la CI après `fsutil behavior set disablelastaccess 0` : témoins (lire un fichier, lister un dossier changent bien la date), puis parcours et MFT → dates d'un dossier, d'un sous-dossier et de deux fichiers inchangées, et le scan rapporte la date d'origine.
- **MFT fragmentée** : au lieu d'une erreur, le parcours prend le relais automatiquement et une ligne discrète l'indique sous la ligne de bilan (« MFT trop fragmentée pour une lecture directe… »). Les autres erreurs de la MFT (UAC refusée, échec de l'assistant) restent des erreurs.

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
8. Gestionnaire des tâches pendant un scan de `C:` : noter la mémoire de Lumen (et de l'assistant pour le scan rapide) et la durée affichée sur la ligne de bilan, avec et sans « Scan rapide (admin) ». Pour un chiffre comparable à la CI : `cd src-tauri && cargo run --release -p scan --example bench -- walk C:\` (et `mft C:\` puis `compare C:\` dans un terminal administrateur).

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
