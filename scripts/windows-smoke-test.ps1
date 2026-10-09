# Starts GetCraft.exe with --smoke-test (window, renderer, tray; no network) and fails unless it
# draws its frames and exits cleanly within the time limit. Prints GetCraft's log either way.
#   pwsh scripts/windows-smoke-test.ps1 path\to\GetCraft.exe
param([Parameter(Mandatory)][string]$Exe, [int]$TimeoutSeconds = 120)
$ErrorActionPreference = "Stop"

$log = Join-Path $env:APPDATA "GetCraft\getcraft.log"
Remove-Item $log -ErrorAction SilentlyContinue
$process = Start-Process -FilePath $Exe -ArgumentList "--smoke-test" -PassThru
$finished = $process.WaitForExit($TimeoutSeconds * 1000)
if (Test-Path $log) { Get-Content $log } else { Write-Host "(no log written)" }
if (-not $finished) {
    $process.Kill()
    throw "GetCraft didn't finish its smoke test within $TimeoutSeconds seconds"
}
if ($process.ExitCode -ne 0) { throw "GetCraft's smoke test failed with exit code $($process.ExitCode)" }
if (-not (Select-String -Path $log -Pattern "smoke test passed" -Quiet)) { throw "GetCraft didn't report a passed smoke test" }
Write-Host "Smoke test passed: $Exe"
