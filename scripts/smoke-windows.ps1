# Run only on a clean Windows build runner. The installed package is retained
# under RUNNER_TEMP for the subsequent packaged-CLI E2E test and diagnostics.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Installer,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,
    [switch]$SkipDesktop
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or -not $env:RUNNER_TEMP) {
    throw 'Windows package smoke testing requires a Windows runner with RUNNER_TEMP.'
}
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
if ([IO.Path]::GetExtension($installerPath) -ne '.exe') {
    throw 'Installer must be the built NSIS .exe package.'
}

# NSIS may offer to remove an older registered install even with /D. Refuse
# such a machine before starting it; never uninstall an existing user package.
$uninstallRoots = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
foreach ($registryRoot in $uninstallRoots) {
    $existing = @(Get-ItemProperty -Path $registryRoot -ErrorAction SilentlyContinue | Where-Object {
        ($_.PSObject.Properties['DisplayName'] -and $_.DisplayName -match '^SAG[ -]?Wiki(?:\s|$)') -or
        $_.PSChildName -in @('com.sag.desktop', 'SAG Wiki', 'sag-desktop')
    })
    if ($existing.Count -gt 0) {
        throw 'An existing SAG Wiki installation was found. Use a clean runner; it has not been changed.'
    }
}

if (-not $SkipDesktop) {
    # Desktop bootstrap manages CLI/Skills in the profile. A pre-created
    # cli-initialized marker below skips first installation on a clean profile.
    $profileDirectory = [Environment]::GetFolderPath('UserProfile')
    foreach ($relative in @('.local\bin\sag.exe', '.claude\skills\sag', '.agents\skills\sag', '.config\opencode\skills\sag')) {
        if (Test-Path -LiteralPath (Join-Path $profileDirectory $relative)) {
            throw 'Desktop smoke testing requires a clean SAG CLI/Skills profile; existing files have not been changed.'
        }
    }
}

$smokeDirectory = Join-Path ([IO.Path]::GetFullPath($env:RUNNER_TEMP)) ('sag-windows-smoke-' + [guid]::NewGuid().ToString('N'))
$installDirectory = Join-Path $smokeDirectory 'installation'
$dataDirectory = Join-Path $smokeDirectory 'data'
New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
$desktopPath = Join-Path $installDirectory 'sag-desktop.exe'
$cliPath = Join-Path $installDirectory 'bin\sag.exe'
$environmentNames = @('SAG_DATA_DIR', 'SAG_DESKTOP_DATA_DIR', 'WEBVIEW2_USER_DATA_FOLDER')
$previousEnvironment = @{}
foreach ($name in $environmentNames) {
    $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$desktopProcess = $null
$installerProcess = $null

try {
    $env:SAG_DATA_DIR = $dataDirectory
    $env:SAG_DESKTOP_DATA_DIR = $dataDirectory
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $smokeDirectory 'webview'
    # /D must be the last NSIS option and must not be quoted, even for spaces.
    # /NS suppresses shortcuts; no /R means the installer cannot launch the app.
    $installerProcess = Start-Process -FilePath $installerPath -ArgumentList "/S /NS /D=$installDirectory" -PassThru
    if (-not $installerProcess.WaitForExit(120000)) {
        throw 'The silent NSIS installation did not finish within 120 seconds.'
    }
    if ($installerProcess.ExitCode -ne 0) {
        throw "The silent NSIS installation failed with exit code $($installerProcess.ExitCode)."
    }
    foreach ($file in @($desktopPath, $cliPath)) {
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
            throw "Installed package is missing $file"
        }
    }
    $desktopVersion = (Get-Item -LiteralPath $desktopPath).VersionInfo.ProductVersion
    if ($desktopVersion -notmatch ('^' + [regex]::Escape($Version) + '(?:\.0)?$')) {
        throw "Desktop executable version $desktopVersion does not match $Version."
    }
    $clipperPath = Join-Path $installDirectory 'clipper\sag-web-clipper-chrome.zip'
    if (-not (Test-Path -LiteralPath $clipperPath)) { throw 'Installed Chrome/Edge extension package is missing.' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $clipperArchive = [IO.Compression.ZipFile]::OpenRead($clipperPath)
    try {
        foreach ($name in @('manifest.json', 'background.js', 'extract.js', 'popup.html', 'options.html')) {
            $entry = $clipperArchive.GetEntry($name)
            if (-not $entry -or $entry.Length -eq 0) { throw "Installed clipper entry is missing: $name" }
        }
    } finally { $clipperArchive.Dispose() }
    $cliVersion = (& $cliPath --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $cliVersion -ne "sag $Version") {
        throw "Installed CLI version check failed: $cliVersion"
    }
    $identity = & $cliPath --build-info | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $identity.version -ne $Version -or $identity.channel -ne 'release') {
        throw 'Installed CLI build identity does not match the requested release.'
    }

    if (-not $SkipDesktop) {
        [IO.File]::WriteAllText((Join-Path $dataDirectory 'cli-initialized'), '1')
        $desktopProcess = Start-Process -FilePath $desktopPath -WorkingDirectory $installDirectory -PassThru `
            -RedirectStandardOutput (Join-Path $smokeDirectory 'desktop.stdout.log') `
            -RedirectStandardError (Join-Path $smokeDirectory 'desktop.stderr.log')
        $readyDeadline = [DateTime]::UtcNow.AddSeconds(30)
        $endpointPath = Join-Path $dataDirectory 'sag-service.json'
        while (-not (Test-Path -LiteralPath $endpointPath)) {
            $desktopProcess.Refresh()
            if ($desktopProcess.HasExited) {
                throw "Installed desktop exited during startup with code $($desktopProcess.ExitCode)."
            }
            if ([DateTime]::UtcNow -gt $readyDeadline) {
                throw 'Installed desktop did not start its isolated SAG service within 30 seconds.'
            }
            Start-Sleep -Milliseconds 250
        }
        Start-Sleep -Seconds 3
        $desktopProcess.Refresh()
        if ($desktopProcess.HasExited) {
            throw "Installed desktop exited immediately after startup with code $($desktopProcess.ExitCode)."
        }
        $service = & $cliPath service status | ConvertFrom-Json
        if ($LASTEXITCODE -ne 0 -or $service.version -ne $Version -or $service.data_directory -ne $dataDirectory) {
            throw 'Installed desktop did not connect to the expected isolated service.'
        }
    }

    $result = [ordered]@{
        status = 'passed'
        version = $Version
        desktop = $desktopPath
        cli = $cliPath
        directory = $smokeDirectory
        desktop_started = -not $SkipDesktop.IsPresent
    }
}
finally {
    # Stop only processes created here, and only the service authenticated by
    # this run's isolated data directory. No executable-name-wide termination.
    try {
        foreach ($process in @($desktopProcess, $installerProcess)) {
            if ($null -ne $process) {
                $process.Refresh()
                if (-not $process.HasExited) {
                    $process.Kill()
                    $null = $process.WaitForExit(10000)
                }
            }
        }
        if ((Test-Path -LiteralPath $cliPath) -and (Test-Path -LiteralPath (Join-Path $dataDirectory 'sag-service.json'))) {
            $stopOutput = & $cliPath service stop | Out-String
            if ($LASTEXITCODE -ne 0) {
                throw "The isolated SAG service could not be stopped: $stopOutput"
            }
        }
    }
    finally {
        foreach ($name in $environmentNames) {
            [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], 'Process')
        }
    }
}

$resultJson = $result | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $smokeDirectory 'smoke-result.json'), $resultJson)
if ($env:GITHUB_OUTPUT) {
    foreach ($entry in @("cli=$cliPath", "desktop=$desktopPath", "directory=$smokeDirectory")) {
        Add-Content -LiteralPath $env:GITHUB_OUTPUT -Value $entry -Encoding utf8
    }
}
Write-Output $resultJson
