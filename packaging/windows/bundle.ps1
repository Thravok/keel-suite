# Bundle Windows release artifacts and build NSIS installers.
# Usage: .\bundle.ps1 -App deck|light -Version 0.1.0 -BinDir path\to\bins -OutDir path\to\out
param(
  [Parameter(Mandatory = $true)][ValidateSet("deck", "light")][string]$App,
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][string]$BinDir,
  [Parameter(Mandatory = $true)][string]$OutDir
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "../..")
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$Stage = Join-Path $OutDir "stage-$App"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null

function Find-WinDeployQt {
  $candidates = @(
    "$env:QT_DIR\bin\windeployqt.exe",
    "$env:CMAKE_PREFIX_PATH\bin\windeployqt.exe"
  )
  foreach ($c in $candidates) {
    if ($c -and (Test-Path $c)) { return $c }
  }
  $cmd = Get-Command windeployqt -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  return $null
}

if ($App -eq "deck") {
  Copy-Item (Join-Path $BinDir "keel.exe") (Join-Path $Stage "keel.exe")
  $Nsi = Join-Path $Root "packaging/windows/keel-deck.nsi"
  $OutName = "KeelDeck-$Version-windows-x64.exe"
} else {
  Copy-Item (Join-Path $BinDir "keel-light.exe") (Join-Path $Stage "keel-light.exe")
  $Ui = Join-Path $BinDir "keel-light-ui.exe"
  if (Test-Path $Ui) {
    Copy-Item $Ui (Join-Path $Stage "keel-light-ui.exe")
    $wdq = Find-WinDeployQt
    if ($wdq) {
      & $wdq --release --no-translations (Join-Path $Stage "keel-light-ui.exe")
    } else {
      Write-Warning "windeployqt not found; Qt DLLs may be missing from installer"
    }
  }
  $Nsi = Join-Path $Root "packaging/windows/keel-light.nsi"
  $OutName = "KeelLight-$Version-windows-x64.exe"
}

$Makensis = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $Makensis) {
  throw "makensis not found — install NSIS and add to PATH"
}

$OutFile = Join-Path $OutDir $OutName
& makensis /DPRODUCT_VERSION=$Version "/DSOURCE_DIR=$Stage" "/XOutFile $OutFile" $Nsi
if ($LASTEXITCODE -ne 0) { throw "makensis failed" }

# Authenticode (optional)
if ($env:WINDOWS_CERTIFICATE_PFX) {
  $Pfx = Join-Path $OutDir "cert.pfx"
  [IO.File]::WriteAllBytes($Pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE_PFX))
  $Pwd = $env:WINDOWS_CERTIFICATE_PASSWORD
  $Signtool = Get-Command signtool -ErrorAction SilentlyContinue
  if ($Signtool) {
    & signtool sign /f $Pfx /p $Pwd /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $OutFile
  } else {
    Write-Warning "signtool not found — installer left unsigned"
  }
  Remove-Item $Pfx -Force -ErrorAction SilentlyContinue
} else {
  Write-Host "WINDOWS_CERTIFICATE_PFX not set — unsigned installer"
}

Write-Host "Built $OutFile"
