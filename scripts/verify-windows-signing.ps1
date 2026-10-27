# Verification only: this script neither signs binaries nor changes certificate
# trust. A Windows signing provider must be connected before raw artifacts pass.
[CmdletBinding()]
param(
    [string]$InputDirectory = 'release-artifacts/windows',
    [string]$OutputDirectory = 'windows-signing-proof',
    [string]$ExpectedPublisher = $env:WINDOWS_SIGNING_PUBLISHER
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-WindowsPublisher([AllowEmptyString()][string]$ExpectedPublisher) {
    if ([string]::IsNullOrWhiteSpace($ExpectedPublisher)) {
        throw 'WINDOWS_SIGNING_PUBLISHER must identify the approved Windows release publisher.'
    }
}

function Assert-WindowsCodeSignature($Signature, [AllowEmptyString()][string]$ExpectedPublisher) {
    Assert-WindowsPublisher $ExpectedPublisher
    if ($null -eq $Signature -or -not $Signature.PSObject.Properties['Status']) {
        throw 'Windows did not return an Authenticode signature status.'
    }
    # Windows validates the signer/timestamp chains and file integrity. Do not
    # replace this with certificate presence or install any extra root CA.
    if ($Signature.Status.ToString() -cne 'Valid') {
        throw "Windows signature status must be Valid; received $($Signature.Status)."
    }
    if (-not $Signature.PSObject.Properties['SignatureType'] -or $Signature.SignatureType.ToString() -cne 'Authenticode') {
        throw 'An embedded Authenticode signature is required; catalog signatures cannot be released.'
    }
    if (-not $Signature.PSObject.Properties['SignerCertificate'] -or
        $Signature.SignerCertificate -isnot [Security.Cryptography.X509Certificates.X509Certificate2]) {
        throw 'A Windows signer certificate is required.'
    }
    $certificate = $Signature.SignerCertificate
    $publisher = $certificate.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
    if (-not [string]::Equals($publisher, $ExpectedPublisher, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Windows certificate publisher does not match WINDOWS_SIGNING_PUBLISHER.'
    }
    $extensions = @($certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' })
    $hasCodeSigningEku = $false
    foreach ($extension in $extensions) {
        $eku = [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($extension, $extension.Critical)
        if (@($eku.EnhancedKeyUsages | Where-Object { $_.Value -eq '1.3.6.1.5.5.7.3.3' }).Count -gt 0) {
            $hasCodeSigningEku = $true
        }
    }
    if (-not $hasCodeSigningEku) { throw 'The signer certificate must contain the code-signing EKU.' }
    if (-not $Signature.PSObject.Properties['TimeStamperCertificate'] -or
        $Signature.TimeStamperCertificate -isnot [Security.Cryptography.X509Certificates.X509Certificate2]) {
        throw 'A trusted timestamp is required on every Windows release signature.'
    }
    $thumbprint = $certificate.Thumbprint
    $timestampThumbprint = $Signature.TimeStamperCertificate.Thumbprint
    if ([string]::IsNullOrWhiteSpace($certificate.Subject) -or
        $thumbprint -notmatch '^[A-Fa-f0-9]{40}$' -or $timestampThumbprint -notmatch '^[A-Fa-f0-9]{40}$') {
        throw 'Windows signature certificates have incomplete subjects or thumbprints.'
    }
    return [ordered]@{
        subject = $certificate.Subject
        thumbprint = $thumbprint.ToUpperInvariant()
        timestamp_thumbprint = $timestampThumbprint.ToUpperInvariant()
        status = 'Valid'
    }
}

function Assert-WindowsExactNames([string[]]$Actual, [string[]]$Expected, [string]$Description) {
    if ($Actual.Count -ne $Expected.Count) { throw "Unexpected $Description." }
    foreach ($name in $Expected) {
        if (@($Actual | Where-Object { $_ -ceq $name }).Count -ne 1) { throw "Unexpected $Description." }
    }
}

function Assert-WindowsReleaseMetadata($Metadata, [string]$Version, [string]$Commit, [string[]]$FileNames, [string]$TriggerCommit = '') {
    if ($Metadata -isnot [pscustomobject]) { throw 'Windows release metadata must be a JSON object.' }
    foreach ($name in @('schema', 'version', 'commit', 'platform', 'target', 'build_identity', 'files')) {
        if (-not ($Metadata.PSObject.Properties.Name -ccontains $name)) { throw "Missing Windows metadata field: $name." }
    }
    if (($Metadata.schema -isnot [int] -and $Metadata.schema -isnot [long]) -or $Metadata.schema -ne 1) {
        throw 'Unsupported Windows metadata schema.'
    }
    if ($Version -cnotmatch '^\d+\.\d+\.\d+$' -or $Metadata.version -isnot [string] -or $Metadata.version -cne $Version) {
        throw 'Windows artifact version does not match the stable source version.'
    }
    if ($Commit -cnotmatch '^[a-f0-9]{40}$' -or $Metadata.commit -isnot [string] -or $Metadata.commit -cne $Commit) {
        throw 'Windows artifact commit does not match the source checkout commit.'
    }
    if ($TriggerCommit -and $Commit -cne $TriggerCommit) { throw 'Checkout must match the triggering commit.' }
    if ($Metadata.platform -cne 'windows') { throw 'Windows artifact platform mismatch.' }
    if ($Metadata.target -cne 'x86_64-pc-windows-msvc') { throw 'Windows artifact target mismatch.' }
    if ($Metadata.build_identity -isnot [pscustomobject] -or
        -not $Metadata.build_identity.PSObject.Properties['version'] -or $Metadata.build_identity.version -cne $Version) {
        throw 'Windows artifact CLI version mismatch.'
    }
    if (-not $Metadata.build_identity.PSObject.Properties['channel'] -or $Metadata.build_identity.channel -cne 'release') {
        throw 'Windows artifacts must contain a release CLI.'
    }
    if ($Metadata.files -isnot [pscustomobject]) { throw 'Windows artifact file list must be a JSON object.' }
    $installerName = "SAG-Wiki-$Version-Windows-x64-setup.exe"
    Assert-WindowsExactNames -Actual @($Metadata.files.PSObject.Properties.Name) -Expected @($installerName) -Description 'Windows artifact file list'
    Assert-WindowsExactNames -Actual $FileNames -Expected @('metadata.json', $installerName) -Description 'Windows staging file list'
    $sha256 = $Metadata.files.$installerName
    if ($sha256 -isnot [string] -or $sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Windows metadata contains an invalid SHA-256.' }
    return $installerName
}

function Assert-WindowsPlainFile([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Windows release executables and metadata must be regular files.'
    }
}

function Assert-WindowsArtifactDigest([string]$Path, [string]$ExpectedSha256) {
    if ($ExpectedSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'Expected artifact SHA-256 is invalid.' }
    Assert-WindowsPlainFile $Path
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $ExpectedSha256) { throw 'Windows raw artifact checksum mismatch.' }
}

function Get-WindowsSignatureEvidence([string]$Path, [string]$ExpectedPublisher) {
    Assert-WindowsPlainFile $Path
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    $detail = Assert-WindowsCodeSignature -Signature $signature -ExpectedPublisher $ExpectedPublisher
    return [ordered]@{
        sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
        subject = $detail.subject
        thumbprint = $detail.thumbprint
        timestamp_thumbprint = $detail.timestamp_thumbprint
        status = $detail.status
    }
}

function ConvertFrom-WindowsSmokeOutput([string]$Output, [string]$Version) {
    # The smoke script writes its final JSON after any native diagnostics. Only
    # accept a complete JSON object ending at the end of the captured output.
    $lines = $Output -split '\r?\n'
    $result = $null
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index].TrimStart().StartsWith('{')) {
            try {
                $candidate = ($lines[$index..($lines.Count - 1)] -join "`n") | ConvertFrom-Json
                if ($candidate -is [pscustomobject]) { $result = $candidate; break }
            } catch { continue }
        }
    }
    if ($null -eq $result) { throw 'Windows smoke did not return final JSON evidence.' }
    foreach ($field in @('status', 'version', 'desktop_started', 'desktop', 'cli', 'directory')) {
        if (-not $result.PSObject.Properties[$field]) { throw "Windows smoke JSON is missing $field." }
    }
    if ($result.status -cne 'passed' -or $result.version -cne $Version) { throw 'Windows smoke did not pass for this release version.' }
    if ($result.desktop_started -isnot [bool] -or -not $result.desktop_started) { throw 'Windows desktop startup must pass before publication.' }
    foreach ($field in @('desktop', 'cli', 'directory')) {
        if ($result.$field -isnot [string] -or [string]::IsNullOrWhiteSpace($result.$field)) { throw "Windows smoke JSON has an invalid $field path." }
    }
    return $result
}

function Invoke-WindowsPackageSmoke([string]$Installer, [string]$Version) {
    $output = @(& (Join-Path $PSScriptRoot 'smoke-windows.ps1') -Installer $Installer -Version $Version) | Out-String
    return ConvertFrom-WindowsSmokeOutput -Output $output -Version $Version
}

function Invoke-WindowsSigningVerification([string]$InputDirectory, [string]$OutputDirectory, [AllowEmptyString()][string]$ExpectedPublisher) {
    Assert-WindowsPublisher $ExpectedPublisher
    if ($env:OS -ne 'Windows_NT' -or [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
        throw 'Windows signing verification requires a clean Windows runner with RUNNER_TEMP.'
    }
    $root = Split-Path -Parent $PSScriptRoot
    $version = (Get-Content -LiteralPath (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
    $desktopVersion = (Get-Content -LiteralPath (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
    if ($version -cne $desktopVersion) { throw 'Desktop source versions must agree before Windows verification.' }
    $commit = (& git -C $root rev-parse HEAD | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Unable to identify the source checkout commit.' }
    $source = [IO.Path]::GetFullPath($InputDirectory)
    $destination = [IO.Path]::GetFullPath($OutputDirectory)
    if (Test-Path -LiteralPath $destination) { throw 'Windows signing proof output directory must be fresh.' }
    $sourcePrefix = $source.TrimEnd([char[]]@([char]92, [char]47)) + [IO.Path]::DirectorySeparatorChar
    if ([string]::Equals($source, $destination, [StringComparison]::OrdinalIgnoreCase) -or
        $destination.StartsWith($sourcePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Windows signing proof output must be outside raw artifact staging.'
    }
    $metadataPath = Join-Path $source 'metadata.json'
    Assert-WindowsPlainFile $metadataPath
    $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
    $names = @(Get-ChildItem -LiteralPath $source -Force | ForEach-Object { $_.Name })
    $installerName = Assert-WindowsReleaseMetadata -Metadata $metadata -Version $version -Commit $commit -TriggerCommit $env:GITHUB_SHA -FileNames $names
    $installerPath = Join-Path $source $installerName
    # Never start an unsigned, untrusted, wrong-publisher or altered installer.
    Assert-WindowsArtifactDigest -Path $installerPath -ExpectedSha256 $metadata.files.$installerName
    $signatures = [ordered]@{installer = Get-WindowsSignatureEvidence -Path $installerPath -ExpectedPublisher $ExpectedPublisher}
    if ($signatures.installer.sha256 -cne $metadata.files.$installerName) { throw 'Windows raw artifact changed during signature verification.' }
    $smoke = Invoke-WindowsPackageSmoke -Installer $installerPath -Version $version
    $smokeRoot = [IO.Path]::GetFullPath($smoke.directory)
    $runnerPrefix = ([IO.Path]::GetFullPath($env:RUNNER_TEMP)).TrimEnd([char[]]@([char]92, [char]47)) + [IO.Path]::DirectorySeparatorChar
    if (-not [IO.Path]::IsPathFullyQualified($smoke.directory) -or
        -not $smokeRoot.StartsWith($runnerPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Windows smoke installation must belong to this runner temporary directory.'
    }
    $installation = Join-Path $smokeRoot 'installation'
    $desktop = Join-Path $installation 'sag-desktop.exe'
    $cli = Join-Path $installation 'bin/sag.exe'
    foreach ($pair in @(@($smoke.desktop, $desktop), @($smoke.cli, $cli))) {
        if (-not [IO.Path]::IsPathFullyQualified($pair[0]) -or
            -not [string]::Equals([IO.Path]::GetFullPath($pair[0]), [IO.Path]::GetFullPath($pair[1]), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Windows smoke returned an unexpected installed executable path.'
        }
    }
    $signatures.desktop = Get-WindowsSignatureEvidence -Path $desktop -ExpectedPublisher $ExpectedPublisher
    $signatures.cli = Get-WindowsSignatureEvidence -Path $cli -ExpectedPublisher $ExpectedPublisher
    $uninstaller = Join-Path $installation 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw 'Installed NSIS uninstall.exe is required before publication.' }
    $signatures.uninstaller = Get-WindowsSignatureEvidence -Path $uninstaller -ExpectedPublisher $ExpectedPublisher
    & python (Join-Path $root 'tests/local-jev-e2e.py') --binary $cli | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Signed installed Windows CLI E2E failed.' }
    # Check again after smoke/E2E, so the receipt always describes the unchanged
    # original installer that the publication job will download and publish.
    Assert-WindowsArtifactDigest -Path $installerPath -ExpectedSha256 $metadata.files.$installerName
    $receipt = [ordered]@{
        schema = 1
        version = $metadata.version
        commit = $metadata.commit
        platform = 'windows'
        target = 'x86_64-pc-windows-msvc'
        files = $metadata.files
        windows_signing = [ordered]@{publisher = $ExpectedPublisher; files = $signatures}
    }
    $parent = Split-Path -Parent $destination
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    $temporary = Join-Path $parent ('.sag-windows-signing-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $temporary | Out-Null
    try {
        $json = ($receipt | ConvertTo-Json -Depth 20) + "`n"
        [IO.File]::WriteAllText((Join-Path $temporary 'windows-signing.json'), $json, [Text.UTF8Encoding]::new($false))
        [IO.Directory]::Move($temporary, $destination)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
    }
    return $receipt
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        $null = Invoke-WindowsSigningVerification -InputDirectory $InputDirectory -OutputDirectory $OutputDirectory -ExpectedPublisher $ExpectedPublisher
        Write-Host "Verified Windows signing proof at $([IO.Path]::GetFullPath($OutputDirectory))."
    } catch {
        Write-Error $_.Exception.Message -ErrorAction Continue
        exit 1
    }
}
