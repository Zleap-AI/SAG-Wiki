# Tauri 2.7's build helper embeds resources only in application binaries.
# Its library test harness also imports comctl32!TaskDialogIndirect, so it
# needs Common Controls v6 before Windows can load it (tauri#13419).
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows) { throw 'This test runner requires Windows and the Windows SDK' }

$root = Split-Path $PSScriptRoot -Parent
$cargoManifest = Join-Path $root 'src-tauri/Cargo.toml'
$testManifest = Join-Path $PSScriptRoot 'windows-test.manifest.xml'
$mt = Get-ChildItem "${env:ProgramFiles(x86)}/Windows Kits/10/bin/*/x64/mt.exe" |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $mt) { throw 'Windows SDK mt.exe is required to embed the test manifest' }

# Use Cargo's artifact records instead of globbing stale or non-test binaries.
$messages = & cargo test --locked --manifest-path $cargoManifest --no-run --message-format=json-render-diagnostics
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$executables = @($messages | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } |
    Select-Object -ExpandProperty executable -Unique)
if ($executables.Count -eq 0) { throw 'Cargo reported no desktop test executables' }
foreach ($executable in $executables) {
    Write-Host "Embed Common Controls v6 in test harness: $executable"
    & $mt -nologo -manifest $testManifest "-outputresource:$executable;#1"
    if ($LASTEXITCODE -ne 0) { throw "Could not embed the manifest in $executable" }
}

# Cargo supplies the test process environment and runs the complete suite.
# Only test harnesses were changed; application and installer outputs are untouched.
& cargo test --locked --manifest-path $cargoManifest
exit $LASTEXITCODE
