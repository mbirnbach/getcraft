# Installs GetCraft silently with the setup program, smoke-tests the installed copy, uninstalls
# it again and checks that it's gone.
#   pwsh scripts/windows-installer-test.ps1 path\to\getcraft-<version>-windows-setup.exe
param([Parameter(Mandatory)][string]$Setup)
$ErrorActionPreference = "Stop"

$dir = Join-Path $env:LOCALAPPDATA "Programs\GetCraft"
$installed = Join-Path $dir "GetCraft.exe"

$p = Start-Process -FilePath $Setup -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-" -PassThru -Wait
if ($p.ExitCode -ne 0) { throw "setup failed with exit code $($p.ExitCode)" }
if (-not (Test-Path $installed)) { throw "setup didn't install $installed" }
$shortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\GetCraft.lnk"
if (-not (Test-Path $shortcut)) { throw "setup didn't create the Start menu shortcut" }
$entry = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7CF358B6-E104-44DA-AC2E-E883DB1AA9F1}_is1"
Write-Host "Installed: $($entry.DisplayName) $($entry.DisplayVersion)"

& (Join-Path $PSScriptRoot "windows-smoke-test.ps1") $installed

$uninstaller = Join-Path $dir "unins000.exe"
$p = Start-Process -FilePath $uninstaller -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -PassThru -Wait
if ($p.ExitCode -ne 0) { throw "uninstall failed with exit code $($p.ExitCode)" }
# The uninstaller finishes in the background after copying itself away; give it a moment.
for ($i = 0; $i -lt 30 -and (Test-Path $installed); $i++) { Start-Sleep -Seconds 1 }
if (Test-Path $installed) { throw "uninstall left $installed behind" }
if (Test-Path $shortcut) { throw "uninstall left the Start menu shortcut behind" }
Write-Host "Install, start and uninstall all worked."
