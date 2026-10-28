; Lumen — ajouts à l'installeur NSIS de Tauri (`bundle.windows.nsis.installerHooks`), insérés par
; installer.nsi (modèle de Tauri adapté, ajouts marqués « LUMEN »).
;
; Installation pour tous les utilisateurs (Program Files : requis par l'assistant MFT, qui refuse
; un exécutable modifiable sans droits d'administrateur). Le menu contextuel et l'entrée Run sont
; dans HKCU, comme ceux que Lumen écrit lui-même (Paramètres) : mêmes clés, mêmes commandes
; (src-tauri/src/integration/registry.rs).
;
; Ligne de commande (installation silencieuse /S, ou /P) : /NOMENU (pas de menu contextuel),
; /NODESKTOP (pas de raccourci Bureau), /AUTOSTART (lancer au démarrage de Windows).
;
; La désinstallation ne touche JAMAIS aux jonctions du déplacement intelligent ni à leurs cibles :
; elle les liste (jonctions.txt, tenu à jour par Lumen) et prévient avant de continuer.

; Clés du menu, relatives à la racine des classes de l'utilisateur.
!define LUMEN_CLASS_DIR "Directory\shell\Lumen"
!define LUMEN_CLASS_BACKGROUND "Directory\Background\shell\Lumen"
!define LUMEN_CLASS_DRIVE "Drive\shell\Lumen"
!define LUMEN_MENU_DIR "Software\Classes\${LUMEN_CLASS_DIR}"
!define LUMEN_MENU_BACKGROUND "Software\Classes\${LUMEN_CLASS_BACKGROUND}"
!define LUMEN_MENU_DRIVE "Software\Classes\${LUMEN_CLASS_DRIVE}"
!define LUMEN_PROFILE_LIST "SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList"
!define LUMEN_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define LUMEN_OPEN_ARG "--lumen-open"
!define LUMEN_BACKGROUND_ARG "--lumen-background"
; Au plus, dans l'avertissement (limite de longueur des chaînes NSIS).
!define LUMEN_JUNCTIONS_SHOWN 5
; Au plus, lues dans chaque liste (fichier modifiable par l'utilisateur).
!define LUMEN_JUNCTIONS_READ 500

!include TextFunc.nsh

${UnStrLoc}

Var LumenMenu
Var LumenDesktop
Var LumenAutostart
Var LumenMenuBox
Var LumenDesktopBox
Var LumenAutostartBox
Var LumenJunctions
Var LumenJunctionCount
Var LumenReport
Var LumenOwnLocal

; --- Registre ---------------------------------------------------------------------------------

!macro LUMEN_WRITE_MENU KEY
  WriteRegStr HKCU "${KEY}" "" "$(lumenMenuLabel)"
  WriteRegStr HKCU "${KEY}" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr HKCU "${KEY}\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" ${LUMEN_OPEN_ARG} $\"%V$\""
!macroend

; Seulement une clé « Lumen » qui lance Lumen (ou sans commande) : jamais celle d'un autre programme.
!macro LUMEN_DELETE_MENU UN KEY
  ReadRegStr $R9 HKCU "${KEY}\command" ""
  ${${UN}StrLoc} $R8 $R9 "${LUMEN_OPEN_ARG}" ">"
  ${If} $R9 == ""
  ${OrIf} $R8 != ""
    DeleteRegKey HKCU "${KEY}"
  ${EndIf}
!macroend

!macro LUMEN_DELETE_RUN UN
  ReadRegStr $R9 HKCU "${LUMEN_RUN_KEY}" "${PRODUCTNAME}"
  ${${UN}StrLoc} $R8 $R9 "${LUMEN_BACKGROUND_ARG}" ">"
  ${If} $R8 != ""
    DeleteRegValue HKCU "${LUMEN_RUN_KEY}" "${PRODUCTNAME}"
  ${EndIf}
!macroend

; --- Installation -----------------------------------------------------------------------------

; .onInit : par défaut menu contextuel et raccourci Bureau, pas de démarrage ; par-dessus une
; installation en place, ce qui y est déjà ; puis la ligne de commande.
!macro LUMEN_INIT
  StrCpy $LumenMenu 1
  StrCpy $LumenDesktop 1
  StrCpy $LumenAutostart 0
  ReadRegStr $R0 SHCTX "${MANUPRODUCTKEY}" ""
  ${If} $R0 != ""
    ReadRegStr $R1 HKCU "${LUMEN_MENU_DIR}\command" ""
    ${StrLoc} $R2 $R1 "${LUMEN_OPEN_ARG}" ">"
    ${If} $R2 == ""
      StrCpy $LumenMenu 0
    ${EndIf}
    ${IfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
      StrCpy $LumenDesktop 0
    ${EndIf}
    ReadRegStr $R1 HKCU "${LUMEN_RUN_KEY}" "${PRODUCTNAME}"
    ${StrLoc} $R2 $R1 "${LUMEN_BACKGROUND_ARG}" ">"
    ${If} $R2 != ""
      StrCpy $LumenAutostart 1
    ${EndIf}
  ${EndIf}
  ${GetOptions} $CMDLINE "/NOMENU" $R0
  ${IfNot} ${Errors}
    StrCpy $LumenMenu 0
  ${EndIf}
  ${GetOptions} $CMDLINE "/NODESKTOP" $R0
  ${IfNot} ${Errors}
    StrCpy $LumenDesktop 0
  ${EndIf}
  ${GetOptions} $CMDLINE "/AUTOSTART" $R0
  ${IfNot} ${Errors}
    StrCpy $LumenAutostart 1
  ${EndIf}
!macroend

!macro LUMEN_PAGE_FUNCTIONS
  Function LumenOptionsPage
    ${If} $PassiveMode = 1
      Abort
    ${EndIf}
    !insertmacro MUI_HEADER_TEXT "$(lumenOptionsTitle)" "$(lumenOptionsSubtitle)"
    nsDialogs::Create 1018
    Pop $R0
    ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

    ${NSD_CreateCheckbox} 0 0 100% 12u "$(lumenOptionMenu)"
    Pop $LumenMenuBox
    ${NSD_CreateLabel} 12u 14u -12u 20u "$(lumenOptionMenuHint)"
    Pop $R0
    ${NSD_CreateCheckbox} 0 42u 100% 12u "$(lumenOptionDesktop)"
    Pop $LumenDesktopBox
    ${NSD_CreateCheckbox} 0 66u 100% 12u "$(lumenOptionAutostart)"
    Pop $LumenAutostartBox
    ${NSD_CreateLabel} 12u 80u -12u 20u "$(lumenOptionAutostartHint)"
    Pop $R0

    ${If} $LumenMenu = 1
      ${NSD_Check} $LumenMenuBox
    ${EndIf}
    ${If} $LumenDesktop = 1
      ${NSD_Check} $LumenDesktopBox
    ${EndIf}
    ${If} $LumenAutostart = 1
      ${NSD_Check} $LumenAutostartBox
    ${EndIf}
    ${NSD_SetFocus} $LumenMenuBox
    nsDialogs::Show
  FunctionEnd

  Function LumenOptionsLeave
    ${NSD_GetState} $LumenMenuBox $LumenMenu
    ${NSD_GetState} $LumenDesktopBox $LumenDesktop
    ${NSD_GetState} $LumenAutostartBox $LumenAutostart
  FunctionEnd
!macroend

; Après la copie des fichiers : chaque choix est appliqué (coché : écrit ; décoché : retiré).
!macro NSIS_HOOK_POSTINSTALL
  ${If} $LumenMenu = ${BST_CHECKED}
    !insertmacro LUMEN_WRITE_MENU "${LUMEN_MENU_DIR}"
    !insertmacro LUMEN_WRITE_MENU "${LUMEN_MENU_BACKGROUND}"
    !insertmacro LUMEN_WRITE_MENU "${LUMEN_MENU_DRIVE}"
  ${Else}
    !insertmacro LUMEN_DELETE_MENU "" "${LUMEN_MENU_DIR}"
    !insertmacro LUMEN_DELETE_MENU "" "${LUMEN_MENU_BACKGROUND}"
    !insertmacro LUMEN_DELETE_MENU "" "${LUMEN_MENU_DRIVE}"
  ${EndIf}

  ${If} $LumenAutostart = ${BST_CHECKED}
    WriteRegStr HKCU "${LUMEN_RUN_KEY}" "${PRODUCTNAME}" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" ${LUMEN_BACKGROUND_ARG}"
  ${Else}
    !insertmacro LUMEN_DELETE_RUN ""
  ${EndIf}

  ${If} $LumenDesktop = ${BST_CHECKED}
    ; Installations silencieuses : déjà créé par le modèle de Tauri.
    ${IfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
      Call CreateOrUpdateDesktopShortcut
    ${EndIf}
  ${Else}
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $R0
    ${If} $R0 = 1
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}
!macroend

; --- Désinstallation --------------------------------------------------------------------------

; Clé du menu dans une ruche de HKEY_USERS : seulement si elle lance Lumen.
!macro LUMEN_DELETE_HKU_MENU KEY
  ReadRegStr $R9 HKU "${KEY}\command" ""
  ${UnStrLoc} $R4 $R9 "${LUMEN_OPEN_ARG}" ">"
  ${If} $R4 != ""
    DeleteRegKey HKU "${KEY}"
  ${EndIf}
!macroend

; Fonctions du désinstalleur, insérées par installer.nsi après les définitions du modèle
; (PRODUCTNAME…).
!macro LUMEN_UN_FUNCTIONS
; Le désinstalleur tourne en administrateur, parfois sous un autre compte que celui qui a activé
; le menu ou le démarrage : toutes les ruches chargées de HKEY_USERS (<SID> et <SID>_Classes)
; sont nettoyées, pas seulement HKCU. Les profils non chargés ne sont pas ouverts (pas de
; reg load) : leurs entrées restent, inertes (voir PROGRESS.md, jalon 11).
Function un.LumenCleanLoadedHives
  StrCpy $R5 0
  ${Do}
    EnumRegKey $R6 HKU "" $R5
    ${If} $R6 == ""
    ${OrIf} $R5 >= 1024
      ${ExitDo}
    ${EndIf}
    IntOp $R5 $R5 + 1
    StrCpy $R7 $R6 "" -8
    ${If} $R7 == "_Classes"
      StrCpy $R8 "$R6"
    ${Else}
      StrCpy $R8 "$R6\Software\Classes"
      ReadRegStr $R9 HKU "$R6\${LUMEN_RUN_KEY}" "${PRODUCTNAME}"
      ${UnStrLoc} $R4 $R9 "${LUMEN_BACKGROUND_ARG}" ">"
      ${If} $R4 != ""
        DeleteRegValue HKU "$R6\${LUMEN_RUN_KEY}" "${PRODUCTNAME}"
      ${EndIf}
    ${EndIf}
    !insertmacro LUMEN_DELETE_HKU_MENU "$R8\${LUMEN_CLASS_DIR}"
    !insertmacro LUMEN_DELETE_HKU_MENU "$R8\${LUMEN_CLASS_BACKGROUND}"
    !insertmacro LUMEN_DELETE_HKU_MENU "$R8\${LUMEN_CLASS_DRIVE}"
  ${Loop}
FunctionEnd

; Lit une liste de jonctions (UTF-16 LE, une par ligne ; chemin sur la pile). Ce fichier est
; modifiable par l'utilisateur : son contenu est seulement compté et affiché (et recopié dans
; le rapport /JUNCTIONSREPORT=), jamais exécuté ni utilisé pour agir.
Function un.LumenReadJunctions
  Exch $R0
  Push $R1
  Push $R2
  Push $R3
  ClearErrors
  FileOpen $R2 "$R0" r
  ${IfNot} ${Errors}
    StrCpy $R3 0
    ${Do}
      ClearErrors
      FileReadUTF16LE $R2 $R1
      ${If} ${Errors}
      ${OrIf} $R3 >= ${LUMEN_JUNCTIONS_READ}
        ${ExitDo}
      ${EndIf}
      IntOp $R3 $R3 + 1
      ${TrimNewLines} "$R1" $R1
      ${If} $R1 != ""
        IntOp $LumenJunctionCount $LumenJunctionCount + 1
        ${If} $LumenReport != ""
          FileWriteUTF16LE $LumenReport "$R1$\r$\n"
        ${EndIf}
        ${If} $LumenJunctionCount <= ${LUMEN_JUNCTIONS_SHOWN}
          StrCpy $R1 $R1 140
          StrCpy $LumenJunctions "$LumenJunctions• $R1$\r$\n"
        ${EndIf}
      ${EndIf}
    ${Loop}
    FileClose $R2
  ${EndIf}
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
FunctionEnd

; Listes de jonctions de CHAQUE profil de la machine (ProfileList, chargé ou non) : le compte qui
; désinstalle d'abord (son %LOCALAPPDATA% peut être redirigé), puis <profil>\AppData\Local.
Function un.LumenFindJunctions
  SetShellVarContext current
  StrCpy $LumenOwnLocal "$LOCALAPPDATA"
  Push "$LOCALAPPDATA\Lumen\jonctions.txt"
  Call un.LumenReadJunctions
  !insertmacro SetContext
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}
  StrCpy $R5 0
  ${Do}
    EnumRegKey $R6 HKLM "${LUMEN_PROFILE_LIST}" $R5
    ${If} $R6 == ""
    ${OrIf} $R5 >= 1024
      ${ExitDo}
    ${EndIf}
    IntOp $R5 $R5 + 1
    ReadRegStr $R7 HKLM "${LUMEN_PROFILE_LIST}\$R6" "ProfileImagePath"
    ${If} $R7 != ""
      ExpandEnvStrings $R7 $R7
      ${If} "$R7\AppData\Local" != $LumenOwnLocal
        Push "$R7\AppData\Local\Lumen\jonctions.txt"
        Call un.LumenReadJunctions
      ${EndIf}
    ${EndIf}
  ${Loop}
  ${If} ${RunningX64}
    SetRegView lastused
  ${EndIf}
FunctionEnd

; un.onInit : jonctions listées par Lumen dans %LOCALAPPDATA%\Lumen\jonctions.txt (UTF-16 LE,
; une par ligne). Si elles existent, la liste est montrée et la désinstallation ne continue
; qu'avec l'accord de l'utilisateur ; elles restent intactes dans tous les cas.
!macroend

!macro LUMEN_UN_INIT
  StrCpy $LumenJunctions ""
  StrCpy $LumenJunctionCount 0
  StrCpy $LumenReport ""
  ; Rapport de ce qui a été lu (tests de l'installeur, support) : le fichier donné, rien d'autre.
  ${GetOptions} $CMDLINE "/JUNCTIONSREPORT=" $R0
  ${IfNot} ${Errors}
  ${AndIf} $R0 != ""
    ClearErrors
    FileOpen $LumenReport "$R0" w
    ${If} ${Errors}
      StrCpy $LumenReport ""
    ${EndIf}
  ${EndIf}
  Call un.LumenFindJunctions
  ${If} $LumenReport != ""
    FileClose $LumenReport
    StrCpy $LumenReport ""
  ${EndIf}

  ${If} $LumenJunctionCount > 0
  ${AndIfNot} ${Silent}
  ${AndIf} $PassiveMode <> 1
  ${AndIf} $UpdateMode <> 1
    ${If} $LumenJunctionCount > ${LUMEN_JUNCTIONS_SHOWN}
      IntOp $R2 $LumenJunctionCount - ${LUMEN_JUNCTIONS_SHOWN}
      ; Le nombre remplace {n} dans notre phrase seulement, jamais dans les lignes lues.
      ${WordReplace} "$(lumenJunctionsMore)" "{n}" "$R2" "+" $R3
      StrCpy $LumenJunctions "$LumenJunctions$R3$\r$\n"
    ${EndIf}
    MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION|MB_DEFBUTTON2 "$(lumenJunctionsWarning)$\r$\n$\r$\n$LumenJunctions$\r$\n$(lumenJunctionsContinue)" /SD IDOK IDOK lumen_junctions_ok
    Quit
    lumen_junctions_ok:
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Mise à jour par-dessus : les choix de l'utilisateur restent en place.
  ${If} $UpdateMode <> 1
    !insertmacro LUMEN_DELETE_MENU "Un" "${LUMEN_MENU_DIR}"
    !insertmacro LUMEN_DELETE_MENU "Un" "${LUMEN_MENU_BACKGROUND}"
    !insertmacro LUMEN_DELETE_MENU "Un" "${LUMEN_MENU_DRIVE}"
    !insertmacro LUMEN_DELETE_RUN "Un"
    Call un.LumenCleanLoadedHives
  ${EndIf}
!macroend

; Données de Lumen (%LOCALAPPDATA%\Lumen : historique, journal, analyses enregistrées) : gardées
; par défaut, supprimées si la case est cochée — sauf s'il reste des jonctions, dont le journal
; permet d'annuler le déplacement. Jamais à travers un point d'analyse (jonction, lien).
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
  ${AndIf} $LumenJunctionCount = 0
    SetShellVarContext current
    System::Call 'kernel32::GetFileAttributesW(w "$LOCALAPPDATA\Lumen") i .r0'
    ${If} $0 <> -1
      IntOp $1 $0 & 0x400
      ${If} $1 = 0
        RMDir /r "$LOCALAPPDATA\Lumen"
      ${EndIf}
    ${EndIf}
    !insertmacro SetContext
  ${EndIf}
!macroend

; --- Textes -----------------------------------------------------------------------------------

!macro LUMEN_LANGSTRINGS
  LangString lumenMenuLabel ${LANG_FRENCH} "Analyser avec Lumen"
  LangString lumenMenuLabel ${LANG_ENGLISH} "Analyze with Lumen"
  LangString lumenOptionsTitle ${LANG_FRENCH} "Options"
  LangString lumenOptionsTitle ${LANG_ENGLISH} "Options"
  LangString lumenOptionsSubtitle ${LANG_FRENCH} "Choisis comment Lumen s'intègre à Windows. Tout se change ensuite dans les Paramètres."
  LangString lumenOptionsSubtitle ${LANG_ENGLISH} "Choose how Lumen fits into Windows. Everything can be changed later in Settings."
  LangString lumenOptionMenu ${LANG_FRENCH} "Ajouter « Analyser avec Lumen » au menu contextuel de l'Explorateur"
  LangString lumenOptionMenu ${LANG_ENGLISH} "Add “Analyze with Lumen” to the Explorer context menu"
  LangString lumenOptionMenuHint ${LANG_FRENCH} "Sur les dossiers, les lecteurs et le fond d'un dossier. Sous Windows 11 : dans « Afficher plus d'options »."
  LangString lumenOptionMenuHint ${LANG_ENGLISH} "On folders, drives and folder backgrounds. On Windows 11: under “Show more options”."
  LangString lumenOptionDesktop ${LANG_FRENCH} "Créer un raccourci sur le Bureau"
  LangString lumenOptionDesktop ${LANG_ENGLISH} "Create a desktop shortcut"
  LangString lumenOptionAutostart ${LANG_FRENCH} "Lancer Lumen au démarrage de Windows"
  LangString lumenOptionAutostart ${LANG_ENGLISH} "Start Lumen with Windows"
  LangString lumenOptionAutostartHint ${LANG_FRENCH} "Entrée de démarrage de ton compte, jamais un service. Lumen démarre dans la zone de notification si l'icône est activée dans les Paramètres."
  LangString lumenOptionAutostartHint ${LANG_ENGLISH} "A startup entry of your account, never a service. Lumen starts in the notification area if the icon is turned on in Settings."
  LangString lumenDeleteData ${LANG_FRENCH} "Supprimer aussi les données de Lumen (historique, journal, réglages)"
  LangString lumenDeleteData ${LANG_ENGLISH} "Also delete Lumen's data (history, journal, settings)"
  LangString lumenJunctionsWarning ${LANG_FRENCH} "Lumen a déplacé ces dossiers vers un autre disque. Ils y restent, reliés par une jonction que la désinstallation ne touche pas, et les données de Lumen sont gardées pour pouvoir annuler ces déplacements plus tard :"
  LangString lumenJunctionsWarning ${LANG_ENGLISH} "Lumen moved these folders to another drive. They stay there, linked by a junction that uninstalling does not touch, and Lumen's data is kept so these moves can be undone later:"
  LangString lumenJunctionsMore ${LANG_FRENCH} "• … et {n} autres"
  LangString lumenJunctionsMore ${LANG_ENGLISH} "• … and {n} more"
  LangString lumenJunctionsContinue ${LANG_FRENCH} "Pour les remettre à leur place d'origine, annule-les d'abord dans Lumen (Analyse › Déplacements). Continuer la désinstallation ?"
  LangString lumenJunctionsContinue ${LANG_ENGLISH} "To put them back where they were, undo them in Lumen first (Analysis › Moves). Continue uninstalling?"
!macroend
