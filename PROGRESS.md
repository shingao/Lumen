# Lumen — avancement

Architecture : `docs/PLAN.md` (validée le 20/10/2026). Un jalon à la fois, arrêt après chacun pour test.

| # | Jalon | État |
|---|---|---|
| 1 | Projet Tauri, UI statique fidèle à la maquette (données mockées), navigation, thèmes, Paramètres et palette en squelette | **fait — validé** |
| 2 | Module `safety/` complet + tests (bloquants en CI) | **fait — validé** |
| 3 | Scanner Rust (parcours parallèle puis MFT), progression, sélecteur de disque réel | **fait — validé** (PR #1 mergée) |
| 4 | Moteur de règles, recommandations et KPI calculés | **fait — validé** (PR #2 mergée) |
| 5 | Scripts, copie, exécution (simulation, confirmation, journal) | **fait — validé** (PR #3 mergée, testé en Sandbox) |
| 6 | Nettoyage avancé : dormants, doublons, gros fichiers, gouffres système | **fait — à tester** (dans Windows Sandbox ou une VM d'abord) |
| 7 | Snapshots et Timeline | à faire |
| 8 | Dev et Jeux | à faire |
| 9 | Déplacement intelligent, recherche instantanée, export de rapport | à faire |
| 10 | Onboarding et astuces contextuelles | à faire |
| 11 | Intégration Windows (menu contextuel, zone de notification, alertes) et installeur | à faire |

---

## Jalon 6 — nettoyage avancé

Nouvelles formes de suppression (doublons, fichiers choisis) et premières commandes lancées en administrateur. **À tester d'abord dans Windows Sandbox ou une VM.**

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
  - **WinSxS** : `Dism.exe /Online /Cleanup-Image /StartComponentCleanup`, **sans `/ResetBase`** (non proposé : il rendrait les mises à jour installées impossibles à désinstaller) ;
  - **Hibernation** : `powercfg /hibernate off` (libère `hiberfil.sys`, mais **plus de démarrage rapide** ni de veille prolongée, dit en clair) ou `/type reduced` (garde le démarrage rapide) ;
  - **Points de restauration** : `vssadmin delete shadows /for=C: /oldest /quiet`, **uniquement le plus ancien, jamais `/all`** ;
  - **Windows.old** : **`cleanmgr` uniquement**, lancé en interactif (`/d C:`) — coche « Installation(s) précédente(s) de Windows ». **Pas de `/sagerun`, donc aucune clé `StateFlags` écrite** dans le registre ;
  - **pagefile.sys** (et swapfile.sys) : **informatif**, sans action ;
  - **point de restauration proposé avant** (activé par défaut) : script PowerShell figé dans le binaire (`-EncodedCommand`, aucun texte variable), **prérequis obligatoire** : s'il n'est pas rempli, rien d'autre n'est lancé et le bilan dit pourquoi. **Limite de Windows d'un point par 24 h** gérée : le script lit la fréquence (`SystemRestorePointCreationFrequency`, 1 440 min par défaut) et, si un point plus récent existe, **sort avec le code 3 que Lumen affiche** (« un point de restauration existe déjà depuis moins de 24 h : il sert de point de retour ») au lieu d'échouer en silence ; protection désactivée (5) ou point non créé (4) : arrêt avant les actions ;
  - **gain mesuré par l'espace libre du lecteur système avant et après**, journalisé comme au jalon 5 (type `system`, commandes, codes de sortie et leur signification).
- **Plan, exécution, journal communs** : `advanced_plan` (requête `dormant` / `largest` / `duplicates` avec des identifiants de nœuds **proposés par la dernière recherche**, ou `system` avec des actions de la liste fermée) → même `cleanup::prepare`, même confirmation (empreinte), même exécution, même journal. Le front n'envoie jamais de chemin. Journal en **schéma v2** (migration automatique) : type de nettoyage, raison d'un arrêt avant les actions, cache des empreintes.
- **Interface** : écran **Nettoyage** à 4 onglets (dormants, doublons, gros fichiers, gouffres système) ; confirmation adaptée aux commandes élevées (prérequis obligatoire, « autorisation administrateur (UAC) », « une fois lancée, une commande système se termine seule ») ; bilan avec « déjà fait » (point de restauration récent) et l'arrêt éventuel ; journal avec le type de nettoyage.
- **Tests** : `safety` 82 tests portables + 41 sous Windows, dont nouveaux : doublons jamais tous les exemplaires, liens physiques jamais des doublons, doublon modifié avant suppression sauté, exemplaire gardé disparu → rien supprimé, commandes élevées seulement si le plan l'autorise, point de restauration obligatoire avant les actions ; **fixtures Windows** : hash sans toucher au dernier accès (volume de test où il est tenu à jour), hash du contenu et des bords, fichier cloud jamais lu, commande élevée et son code de sortie, script du point de restauration exécuté, gouffres lus sans admin. Appli : dormants (base de date, exclusions), gros fichiers, doublons (liens physiques, cloud, applis, cache, annulation), sélections → autorisations, actions système (jamais `/ResetBase` ni `/all`), journal v2 (migration, cache, bilans avec arrêt). Interface (vitest). **Bout en bout sur la CI Windows** : faux profil analysé, doublons trouvés par lecture réelle (lien physique, projet git et AppData écartés, dernier accès inchangé, cache), sélection de tous les exemplaires refusée, une copie supprimée (Corbeille) et l'autre **modifiée juste avant la suppression puis sautée**, dormant supprimé, le reste intact.

### Décisions

- **Gouffres système par `ShellExecuteEx` « runas » plutôt que par l'assistant élevé** (demande à la validation du jalon 5) : l'assistant MFT reste muet, éphémère et mono-tâche. Conséquences assumées : **un processus élevé ne peut pas être arrêté** par Lumen non élevé (annuler ou le délai maximal ne font que cesser d'attendre, ce que le bilan dit), et **ses sorties ne sont pas capturables** : le journal garde le code de sortie et sa signification ; le gain est mesuré sur le disque.
- **cleanmgr en interactif plutôt que `/sagerun`** : pas de clé `StateFlags` à écrire puis retirer dans `HKLM\…\VolumeCaches`, l'utilisateur voit et coche lui-même « Installation(s) précédente(s) de Windows ».
- **Point de restauration par un script PowerShell figé** (Checkpoint-Computer) passé en `-EncodedCommand` : aucun texte variable, binaire par son chemin complet, mêmes règles que les autres commandes élevées. Code 3 = un point récent existe : **compte comme rempli**.
- **Empreinte = BLAKE3(taille ‖ contenu)** ; les bords (64 Kio + 64 Kio) ne servent qu'à écarter vite les fichiers différents, jamais à décider d'un doublon.
- **Doublons : seuil 1 Mo par défaut** (Paramètres), au plus 500 groupes affichés (les totaux restent exacts). Dormants : seuil 1 Mo.
- **Rien n'est coché par défaut** dans les quatre onglets : tout y est « À vérifier ». Pas de hachures (réservées au « sans risque »).
- Les sélections ne valent que pour la dernière recherche de l'analyse affichée : un identifiant qui n'en vient pas est refusé.

### Écarts avec la maquette

- La maquette n'a pas d'écran Nettoyage : il prolonge sa DA (même conteneur et même en-tête que l'Explorer, contrôles segmentés, listes à bord fin, chemins et tailles en mono, pastille « À vérifier » ambre, bouton vert « Nettoyer »). Captures : `design/captures/jalon-6/` (17 à 22).

### Test manuel (jalon 6) — dans Windows Sandbox ou une VM

1. Dans la Sandbox : `fsutil behavior query disablelastaccess`, puis analyse le profil et ouvre **Nettoyage › Fichiers dormants** : la phrase sous le choix des mois cite la même valeur et dit sur quelle date Lumen se base. Change-la (`fsutil behavior set disablelastaccess 1`, redémarrage de Lumen) : la bascule sur la date de modification est annoncée.
2. Crée un vieux fichier (`(Get-Item x.zip).LastWriteTime = (Get-Date).AddYears(-1)`) dans Documents, un autre dans un dossier contenant `.git` et un dans `AppData` : seul le premier est proposé, rien n'est coché.
3. **Doublons** : copie une vidéo à deux endroits, crée un lien physique (`New-Item -ItemType HardLink`) vers l'original. « Rechercher les doublons » : un groupe de 3, le lien n'y est pas ; la date de dernier accès des fichiers est inchangée (`(Get-Item f).LastAccessTime` avant / après).
4. Essaie de cocher tous les exemplaires d'un groupe : le dernier ne se coche pas. « Cocher les copies », puis modifie une copie avant de confirmer : elle est sautée (« contenu modifié »), l'autre part à la Corbeille.
5. Relance la recherche : elle est quasi immédiate (empreintes en cache).
6. **Gros fichiers** : le filtre par type fonctionne, rien n'est coché ; nettoie un fichier, il est dans la Corbeille et au journal.
7. **Gouffres système** : coche WinSxS et « Réduire » l'hibernation, garde le point de restauration : la confirmation annonce une demande UAC par commande. Confirme : UAC pour le point de restauration, puis pour chaque commande ; le bilan montre le gain mesuré sur C:.
8. Relance aussitôt : le point de restauration est signalé « déjà fait » (moins de 24 h), les actions continuent.
9. Refuse l'UAC d'une commande : elle est « refusée (UAC) », rien d'autre ne casse. Désactive la protection du système puis relance avec le point de restauration : arrêt avant les actions, raison affichée.
10. Windows.old (si présent) : le Nettoyage de disque s'ouvre ; aucune clé `StateFlags` n'apparaît sous `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches`.

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
