# Tests Pester (5) du script PowerShell généré par Lumen, exécuté HORS de safety/ sur de vraies
# fixtures : simulation par défaut, Corbeille, fichier modifié après la génération, jonction
# piégée (élément, parent, contenu), script expiré. LUMEN_SCRIPT_GEN = exemple `script` compilé.

BeforeAll {
  $Gen = $env:LUMEN_SCRIPT_GEN
  if (-not $Gen -or -not (Test-Path $Gen)) { throw "LUMEN_SCRIPT_GEN introuvable : $Gen" }

  function New-Fixture {
    $root = Join-Path ([IO.Path]::GetTempPath()) ("lumen-pester-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    $cache = Join-Path $root 'cache'
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    return @{ Root = $root; Cache = $cache; Script = (Join-Path $root 'lumen.ps1'); Journal = (Join-Path $root 'journal.log') }
  }

  function New-File([string]$Path, [string]$Content = 'lumen') {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
    Set-Content -LiteralPath $Path -Value $Content -NoNewline
  }

  function New-LumenScript($F, [int]$AgeHours, [string[]]$Items, [string]$Mode = 'corbeille') {
    & $Gen $F.Cache $F.Script $AgeHours $Mode @Items | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "génération du script : code $LASTEXITCODE" }
  }

  function Invoke-LumenScript($F, [switch]$Executer) {
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $F.Script, '-Journal', $F.Journal)
    if ($Executer) { $arguments += '-Executer' }
    & powershell.exe @arguments | Out-Host
    return $LASTEXITCODE
  }

  function Get-RecycleCount { return (New-Object -ComObject Shell.Application).NameSpace(10).Items().Count }

  function New-Junction([string]$Link, [string]$Target) {
    New-Item -ItemType Junction -Path $Link -Target $Target | Out-Null
  }
}

Describe 'Script PowerShell de Lumen' {
  It 'simule par défaut : rien ne bouge, tout est journalisé' {
    $f = New-Fixture
    New-File (Join-Path $f.Cache 'a.txt')
    New-File (Join-Path $f.Cache 'dossier\x.txt')
    New-LumenScript $f 0 @('a.txt', 'dossier')
    $text = Get-Content -Raw -LiteralPath $f.Script
    $text | Should -Match 'SIMULATION PAR DÉFAUT'
    $text | Should -Not -Match 'Remove-Item'
    Invoke-LumenScript $f | Should -Be 0
    Join-Path $f.Cache 'a.txt' | Should -Exist
    Join-Path $f.Cache 'dossier\x.txt' | Should -Exist
    $log = Get-Content -Raw -LiteralPath $f.Journal
    $log | Should -Match '\[simulation\] À LA CORBEILLE'
    $log | Should -Match 'Bilan : 0 fait\(s\), 2 simulé\(s\)'
  }

  It 'envoie à la Corbeille avec -Executer, jamais de suppression définitive' {
    $f = New-Fixture
    New-File (Join-Path $f.Cache 'a.txt')
    New-File (Join-Path $f.Cache 'dossier\sous\x.txt')
    New-LumenScript $f 0 @('a.txt', 'dossier')
    $before = Get-RecycleCount
    Invoke-LumenScript $f -Executer | Should -Be 0
    Join-Path $f.Cache 'a.txt' | Should -Not -Exist
    Join-Path $f.Cache 'dossier' | Should -Not -Exist
    (Get-RecycleCount) | Should -Be ($before + 2)
    Get-Content -Raw -LiteralPath $f.Journal | Should -Match 'Bilan : 2 fait\(s\)'
  }

  It 'refuse un fichier modifié après la génération (taille ou date)' {
    $f = New-Fixture
    New-File (Join-Path $f.Cache 'taille.txt')
    New-File (Join-Path $f.Cache 'date.txt')
    New-File (Join-Path $f.Cache 'dossier\x.txt')
    New-LumenScript $f 0 @('taille.txt', 'date.txt', 'dossier')
    Add-Content -LiteralPath (Join-Path $f.Cache 'taille.txt') -Value '!'
    (Get-Item (Join-Path $f.Cache 'date.txt')).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddMinutes(-5)
    New-File (Join-Path $f.Cache 'dossier\nouveau.txt')
    Invoke-LumenScript $f -Executer | Should -Be 0
    Join-Path $f.Cache 'taille.txt' | Should -Exist
    Join-Path $f.Cache 'date.txt' | Should -Exist
    Join-Path $f.Cache 'dossier\nouveau.txt' | Should -Exist
    $log = Get-Content -Raw -LiteralPath $f.Journal
    $log | Should -Match 'REFUSÉ \(taille ou date différente de la génération\) : .*taille\.txt'
    $log | Should -Match 'REFUSÉ \(taille ou date différente de la génération\) : .*date\.txt'
    $log | Should -Match 'REFUSÉ \(contenu différent de la génération\) : .*dossier'
  }

  It 'refuse une jonction piégée : élément, parent ou contenu' {
    $f = New-Fixture
    $victime = Join-Path $f.Root 'victime'
    New-File (Join-Path $victime 'precieux.txt') 'ne pas toucher'
    New-File (Join-Path $f.Cache 'element\x.txt')
    New-File (Join-Path $f.Cache 'parent\f.txt')
    New-File (Join-Path $f.Cache 'contenu\y.txt')
    New-LumenScript $f 0 @('element', 'parent\f.txt', 'contenu')
    # 1. Le dossier prévu est remplacé par une jonction vers la victime.
    Remove-Item -Recurse -Force (Join-Path $f.Cache 'element')
    New-Junction (Join-Path $f.Cache 'element') $victime
    # 2. Le parent du fichier prévu devient une jonction vers une copie identique (même taille, même date).
    Move-Item (Join-Path $f.Cache 'parent') (Join-Path $f.Root 'parent-reel')
    New-Junction (Join-Path $f.Cache 'parent') (Join-Path $f.Root 'parent-reel')
    # 3. Une jonction apparaît dans un dossier prévu.
    New-Junction (Join-Path $f.Cache 'contenu\piege') $victime
    Invoke-LumenScript $f -Executer | Should -Be 0
    Join-Path $victime 'precieux.txt' | Should -Exist
    Join-Path $f.Root 'parent-reel\f.txt' | Should -Exist
    Join-Path $f.Cache 'contenu\y.txt' | Should -Exist
    $log = Get-Content -Raw -LiteralPath $f.Journal
    $log | Should -Match 'REFUSÉ \(lien, jonction ou fichier cloud sur le chemin\) : .*element'
    $log | Should -Match 'REFUSÉ \(lien, jonction ou fichier cloud sur le chemin\) : .*parent\\f\.txt'
    $log | Should -Match 'REFUSÉ \(contient un lien ou un fichier cloud\) : .*contenu'
    $log | Should -Match 'Bilan : 0 fait\(s\)'
  }

  It 'refuse de s''exécuter au-delà de 24 h' {
    $f = New-Fixture
    New-File (Join-Path $f.Cache 'a.txt')
    New-LumenScript $f 25 @('a.txt')
    Invoke-LumenScript $f -Executer | Should -Be 2
    Join-Path $f.Cache 'a.txt' | Should -Exist
    Get-Content -Raw -LiteralPath $f.Journal | Should -Match 'REFUS : script généré le .* il expire au bout de 24 h'
  }

  It 'suppression définitive (option) : sans jamais entrer dans un lien' {
    $f = New-Fixture
    New-File (Join-Path $f.Cache 'dossier\sous\x.txt')
    New-LumenScript $f 0 @('dossier') 'definitif'
    Get-Content -Raw -LiteralPath $f.Script | Should -Match "\`$Mode = 'Definitif'"
    Invoke-LumenScript $f -Executer | Should -Be 0
    Join-Path $f.Cache 'dossier' | Should -Not -Exist
    Get-Content -Raw -LiteralPath $f.Journal | Should -Match 'SUPPRIMÉ DÉFINITIVEMENT'
  }
}
