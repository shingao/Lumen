# Signature Authenticode d'un binaire (placeholder, voir docs/SIGNATURE.md). Appelé par Tauri pour
# chaque fichier à signer quand `bundle.windows.signCommand` est fourni : lumen.exe, puis le
# désinstalleur et l'installeur. Sans certificat, échoue : jamais de binaire « à moitié signé ».
#
#   LUMEN_SIGN_PFX        chemin du certificat (.pfx)
#   LUMEN_SIGN_PASSWORD   mot de passe du certificat
#   LUMEN_SIGN_TIMESTAMP  serveur d'horodatage RFC 3161 (par défaut DigiCert)

param([Parameter(Mandatory)][string]$File)

$ErrorActionPreference = "Stop"
if (-not $env:LUMEN_SIGN_PFX -or -not (Test-Path $env:LUMEN_SIGN_PFX)) {
  throw "LUMEN_SIGN_PFX : certificat de signature absent"
}
$timestamp = if ($env:LUMEN_SIGN_TIMESTAMP) { $env:LUMEN_SIGN_TIMESTAMP } else { "http://timestamp.digicert.com" }
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" | Sort-Object FullName | Select-Object -Last 1
if (-not $signtool) { throw "signtool.exe introuvable (Windows SDK)" }
& $signtool.FullName sign /f $env:LUMEN_SIGN_PFX /p $env:LUMEN_SIGN_PASSWORD /fd SHA256 /tr $timestamp /td SHA256 $File
if ($LASTEXITCODE -ne 0) { throw "signature refusée : $File" }
& $signtool.FullName verify /pa $File
if ($LASTEXITCODE -ne 0) { throw "signature invalide : $File" }
