# Fails if an executable needs DLLs that aren't part of every Windows installation, such as the
# Visual C++ runtime (VCRUNTIME140.dll). GetCraft links the C runtime statically
# (.cargo/config.toml), so it must start on a fresh Windows.
#   pwsh scripts/check-windows-deps.ps1 path\to\GetCraft.exe [...]
param([Parameter(Mandatory, ValueFromRemainingArguments)][string[]]$Exes)
$ErrorActionPreference = "Stop"

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -property installationPath
$dumpbin = Get-ChildItem "$vs\VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" | Select-Object -First 1
if (-not $dumpbin) { throw "dumpbin.exe not found" }

$failed = $false
foreach ($exe in $Exes) {
    $deps = & $dumpbin.FullName /nologo /dependents $exe | Where-Object { $_ -match '^\s+\S+\.dll\s*$' } | ForEach-Object { $_.Trim() }
    Write-Host "$exe needs: $($deps -join ', ')"
    $bad = $deps | Where-Object { $_ -match '^(vcruntime|msvcp|msvcr|concrt|vccorlib|ucrtbase)|^api-ms-win-crt-' }
    if ($bad) {
        Write-Host "::error::$exe depends on the C runtime DLLs: $($bad -join ', ')"
        $failed = $true
    }
}
if ($failed) { exit 1 }
