param([string]$Key, [string]$Url, [string]$Model, [switch]$KeyStdin)
$ErrorActionPreference = 'Stop'
$binary = if ($env:SGREP_BINARY) { $env:SGREP_BINARY } else { Join-Path $PSScriptRoot 'sgrep.exe' }
if (-not (Test-Path $binary -PathType Leaf)) { throw 'Run this installer beside the sgrep.exe release binary, or set SGREP_BINARY.' }
& $binary install cli
if ($LASTEXITCODE -ne 0) { throw 'sgrep installation failed.' }
$configArgs = @('config', '--if-needed')
if ($PSBoundParameters.ContainsKey('Key')) { $configArgs += @('--key', $Key) }
if ($PSBoundParameters.ContainsKey('Url')) { $configArgs += @('--url', $Url) }
if ($PSBoundParameters.ContainsKey('Model')) { $configArgs += @('--model', $Model) }
if ($KeyStdin) { $configArgs += '--key-stdin' }
if ($configArgs.Count -gt 2 -or (-not [Console]::IsInputRedirected -and -not [Console]::IsOutputRedirected)) {
    & $binary @configArgs
    if ($LASTEXITCODE -ne 0) { throw 'sgrep configuration failed.' }
}
else { Write-Host 'Open a terminal and run sgrep to complete first-use setup.' }
Write-Host 'Installed sgrep. Open a new terminal to use it.'
