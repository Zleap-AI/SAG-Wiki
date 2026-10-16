$ErrorActionPreference = 'Stop'
$binary = if ($env:SGREP_BINARY) { $env:SGREP_BINARY } else { Join-Path $PSScriptRoot 'sgrep.exe' }
if (-not (Test-Path $binary -PathType Leaf)) { throw 'Run this installer beside the sgrep.exe release binary, or set SGREP_BINARY.' }
& $binary install cli
if ($LASTEXITCODE -ne 0) { throw 'sgrep installation failed.' }
Write-Host 'Installed sgrep. Open a new terminal, then run: sgrep config'
