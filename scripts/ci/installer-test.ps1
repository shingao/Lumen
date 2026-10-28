# Test réel de l'installeur NSIS (CI Windows, administrateur) : installation pour tous les
# utilisateurs, menu contextuel et démarrage dans HKCU, raccourcis, lancement, instance unique
# (un second lancement transmet son dossier et s'arrête), mise à jour par-dessus (réglages et
# données gardés), désinstallation propre qui ne touche ni à une jonction du déplacement
# intelligent ni aux données de Lumen. Puis une installation sans options.
#
#   pwsh scripts/ci/installer-test.ps1 -Installer <chemin du setup .exe>

param([Parameter(Mandatory)][string]$Installer)

$ErrorActionPreference = "Stop"
$InstallDir = Join-Path $env:ProgramFiles "Lumen"
$Exe = Join-Path $InstallDir "lumen.exe"
$Data = Join-Path $env:LOCALAPPDATA "Lumen"
$MenuKeys = @(
  "HKCU:\Software\Classes\Directory\shell\Lumen",
  "HKCU:\Software\Classes\Directory\Background\shell\Lumen",
  "HKCU:\Software\Classes\Drive\shell\Lumen"
)
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$UninstallKey = "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumen"
$StartMenu = Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\Lumen.lnk"
$Desktop = Join-Path $env:PUBLIC "Desktop\Lumen.lnk"

function Check([bool]$ok, [string]$what) {
  if (-not $ok) { throw "ÉCHEC : $what" }
  Write-Host "ok  $what"
}

function Install([string[]]$Arguments) {
  $p = Start-Process -FilePath $Installer -ArgumentList $Arguments -Wait -PassThru
  Check ($p.ExitCode -eq 0) "installeur $($Arguments -join ' ') (code $($p.ExitCode))"
}

function Uninstall {
  # Le désinstalleur se recopie dans %TEMP% et se relance : on attend la fin réelle.
  Start-Process -FilePath (Join-Path $InstallDir "uninstall.exe") -ArgumentList "/S" -Wait
  $deadline = (Get-Date).AddSeconds(90)
  while ((Test-Path $Exe) -or (Test-Path $UninstallKey)) {
    if ((Get-Date) -gt $deadline) { throw "ÉCHEC : la désinstallation n'a pas fini" }
    Start-Sleep -Milliseconds 500
  }
}

function MenuCommand([string]$key) {
  (Get-ItemProperty -Path "$key\command" -ErrorAction SilentlyContinue)."(default)"
}

function LumenProcesses { @(Get-Process -Name "lumen" -ErrorAction SilentlyContinue) }

function StopLumen {
  LumenProcesses | Stop-Process -Force
  Start-Sleep -Seconds 2
}

# --- 1. Installation silencieuse, avec le démarrage de Windows ------------------------------------
Install @("/S", "/AUTOSTART")
Check (Test-Path $Exe) "lumen.exe dans Program Files"
Check (Test-Path $UninstallKey) "entrée de désinstallation (HKLM, tous les utilisateurs)"
Check (Test-Path $StartMenu) "raccourci du menu Démarrer"
Check (Test-Path $Desktop) "raccourci Bureau (coché par défaut)"
foreach ($key in $MenuKeys) {
  $cmd = MenuCommand $key
  Check ($cmd -eq "`"$Exe`" --lumen-open `"%V`"") "menu contextuel $key → $cmd"
}
$run = (Get-ItemProperty -Path $RunKey -ErrorAction SilentlyContinue).Lumen
Check ($run -eq "`"$Exe`" --lumen-background") "entrée Run de HKCU → $run"

# --- 2. Lancement et instance unique -----------------------------------------------------------
StopLumen
Start-Process -FilePath $Exe
Start-Sleep -Seconds 15
Check ((LumenProcesses).Count -eq 1) "Lumen tourne (une instance)"
Check (Test-Path (Join-Path $Data "lumen.db")) "base SQLite créée au démarrage (migrations passées)"
Check (Test-Path (Join-Path $Data "jonctions.txt")) "liste des jonctions écrite pour la désinstallation"
# Second lancement, comme depuis l'Explorateur : transmis à l'instance ouverte, puis arrêté.
$second = Start-Process -FilePath $Exe -ArgumentList @("--lumen-open", "`"$env:SystemRoot`"") -PassThru
Check ($second.WaitForExit(20000)) "le second lancement s'arrête"
Check ((LumenProcesses).Count -eq 1) "toujours une seule instance"
$malicious = Start-Process -FilePath $Exe -ArgumentList @("--lumen-open", "`"C:\a`" & calc.exe & `"`"") -PassThru
Check ($malicious.WaitForExit(20000)) "un chemin piégé est refusé, rien n'est lancé"
Check (@(Get-Process -Name "calc", "CalculatorApp" -ErrorAction SilentlyContinue).Count -eq 0) "aucun programme lancé"
StopLumen

# --- 3. Mise à jour par-dessus ------------------------------------------------------------------
$dbHash = (Get-FileHash (Join-Path $Data "lumen.db")).Hash
$webview = Join-Path $env:LOCALAPPDATA "com.lumen.disk"
$hadWebview = Test-Path $webview
Install @("/S")
Check (Test-Path $Exe) "mise à jour installée"
Check ((Get-FileHash (Join-Path $Data "lumen.db")).Hash -eq $dbHash) "base SQLite gardée telle quelle"
Check ((-not $hadWebview) -or (Test-Path $webview)) "réglages de l'interface (WebView2) gardés"
Check ((MenuCommand $MenuKeys[0]) -like "*--lumen-open*") "menu contextuel gardé"
Check (((Get-ItemProperty -Path $RunKey -ErrorAction SilentlyContinue).Lumen) -like "*--lumen-background*") "démarrage gardé"
Start-Process -FilePath $Exe
Start-Sleep -Seconds 12
Check ((LumenProcesses).Count -eq 1) "Lumen redémarre après la mise à jour"
StopLumen

# --- 4. Jonction d'un déplacement intelligent, puis désinstallation -------------------------------
$target = Join-Path $env:RUNNER_TEMP "lumen-deplace\Cible"
$junction = Join-Path $env:RUNNER_TEMP "lumen-deplace\Vidéos"
New-Item -ItemType Directory -Force -Path $target | Out-Null
Set-Content -Path (Join-Path $target "film.txt") -Value "à garder"
New-Item -ItemType Junction -Path $junction -Target $target | Out-Null
[IO.File]::WriteAllText((Join-Path $Data "jonctions.txt"), "$junction  →  $target`r`n", [Text.UnicodeEncoding]::new($false, $false))
Uninstall
Check (-not (Test-Path $Exe)) "fichiers de Lumen retirés"
Check (-not (Test-Path $StartMenu)) "raccourci du menu Démarrer retiré"
Check (-not (Test-Path $Desktop)) "raccourci Bureau retiré"
foreach ($key in $MenuKeys) { Check (-not (Test-Path $key)) "menu contextuel retiré : $key" }
Check ($null -eq (Get-ItemProperty -Path $RunKey -ErrorAction SilentlyContinue).Lumen) "entrée Run retirée"
$item = Get-Item -LiteralPath $junction -Force
Check (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) "la jonction est intacte"
Check ((Get-Content (Join-Path $junction "film.txt")) -eq "à garder") "le contenu déplacé est intact"
Check (Test-Path (Join-Path $Data "lumen.db")) "données de Lumen gardées (par défaut)"

# --- 5. Installation sans options --------------------------------------------------------------
Install @("/S", "/NOMENU", "/NODESKTOP")
foreach ($key in $MenuKeys) { Check (-not (Test-Path $key)) "pas de menu contextuel : $key" }
Check (-not (Test-Path $Desktop)) "pas de raccourci Bureau"
Check ($null -eq (Get-ItemProperty -Path $RunKey -ErrorAction SilentlyContinue).Lumen) "pas de démarrage"
Uninstall
Check (-not (Test-Path $Exe)) "désinstallé"
Write-Host "Installeur : tout est conforme."
