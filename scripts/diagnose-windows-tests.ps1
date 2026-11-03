$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$evidence = Join-Path $env:GITHUB_WORKSPACE 'windows-loader-evidence'
New-Item -ItemType Directory -Force $evidence | Out-Null
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --no-run --message-format=json > "$evidence/build.jsonl"
if ($LASTEXITCODE -ne 0) { throw 'Could not compile the Windows library test' }
$test = Get-Content "$evidence/build.jsonl" | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } | Select-Object -Last 1 -ExpandProperty executable
if (-not $test) { throw 'No test executable was reported' }
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$dumpbin = & $vswhere -latest -products '*' -find 'VC/Tools/MSVC/**/bin/Hostx64/x64/dumpbin.exe' | Select-Object -First 1
$mt = Get-ChildItem "${env:ProgramFiles(x86)}/Windows Kits/10/bin/*/x64/mt.exe" | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $dumpbin -or -not $mt) { throw 'Windows SDK diagnostic tools are missing' }
& $dumpbin /imports $test > "$evidence/imports.txt"
& $mt "-inputresource:$test;#1" "-out:$evidence/original.manifest" 2>&1 | Out-File "$evidence/manifest-extraction.txt"
Copy-Item $test "$evidence/original-test.exe"
& $test --list 2>&1 | Out-File "$evidence/original-start.txt"
$originalExit = $LASTEXITCODE
$manifest = Join-Path $evidence 'controls-v6.manifest'
@'
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" /></dependentAssembly></dependency>
</assembly>
'@ | Set-Content -Encoding utf8 $manifest
& $mt -manifest $manifest "-outputresource:$test;#1"
if ($LASTEXITCODE -ne 0) { throw 'Could not embed the diagnostic manifest' }
& $test --list 2>&1 | Out-File "$evidence/with-manifest-start.txt"
$patchedExit = $LASTEXITCODE
[ordered]@{ executable=$test; originalExit=$originalExit; withManifestExit=$patchedExit; dumpbin=$dumpbin; mt=$mt } | ConvertTo-Json | Tee-Object -FilePath "$evidence/result.json"
Get-Content "$evidence/manifest-extraction.txt"
Select-String -Path "$evidence/imports.txt" -Pattern 'COMCTL32|TaskDialogIndirect' -Context 2,2
throw 'Diagnostic run completed; stop before production installer assembly'
