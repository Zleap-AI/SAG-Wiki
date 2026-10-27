# These tests exercise release policy with in-memory certificates. The synthetic
# Valid status is a boundary fixture, never evidence of public-CA/Windows trust.
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'verify-windows-signing.ps1')

$script:passed = 0
$script:failed = 0
function Assert-Equal($Actual, $Expected) {
    if ($Actual -cne $Expected) { throw "Expected '$Expected'; received '$Actual'." }
}
function Assert-Throws([scriptblock]$Action, [string]$MessagePattern) {
    try { & $Action | Out-Null }
    catch {
        if ($_.Exception.Message -notmatch $MessagePattern) {
            throw "Unexpected rejection: $($_.Exception.Message)"
        }
        return
    }
    throw 'Expected the release gate to reject this input.'
}
function Test-Case([string]$Name, [scriptblock]$Body) {
    try {
        & $Body
        $script:passed++
        Write-Host "PASS $Name"
    } catch {
        $script:failed++
        Write-Host "FAIL $Name`: $($_.Exception.Message)"
    }
}
function New-PolicyCertificate([string]$Publisher, [string]$Eku) {
    $key = [Security.Cryptography.RSA]::Create(2048)
    try {
        $request = [Security.Cryptography.X509Certificates.CertificateRequest]::new(
            "CN=$Publisher", $key, [Security.Cryptography.HashAlgorithmName]::SHA256,
            [Security.Cryptography.RSASignaturePadding]::Pkcs1)
        if ($Eku) {
            $oids = [Security.Cryptography.OidCollection]::new()
            $null = $oids.Add([Security.Cryptography.Oid]::new($Eku))
            $request.CertificateExtensions.Add(
                [Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($oids, $false))
        }
        return $request.CreateSelfSigned([DateTimeOffset]::UtcNow.AddDays(-1), [DateTimeOffset]::UtcNow.AddDays(7))
    } finally { $key.Dispose() }
}
function New-SignatureFixture {
    return [pscustomobject]@{
        Status = 'Valid'
        StatusMessage = 'Synthetic policy fixture; OS trust is not evaluated.'
        SignatureType = 'Authenticode'
        SignerCertificate = $signer
        TimeStamperCertificate = $timestamp
    }
}
function Copy-Metadata {
    return $metadata | ConvertTo-Json -Depth 20 | ConvertFrom-Json
}
function Assert-Metadata($Value, [string[]]$Names = @('metadata.json', 'SAG-Wiki-0.2.4-Windows-x64-setup.exe')) {
    return Assert-WindowsReleaseMetadata -Metadata $Value -Version '0.2.4' -Commit ('a' * 40) -FileNames $Names
}
function New-WorkflowFixture([string]$Name, [bool]$IncludeUninstaller = $true) {
    $directory = Join-Path $temporary $Name
    $inputDirectory = Join-Path $directory 'input'
    $installation = Join-Path $directory 'smoke/installation'
    New-Item -ItemType Directory -Path $inputDirectory, (Join-Path $installation 'bin') | Out-Null
    $root = Split-Path -Parent $PSScriptRoot
    $version = (Get-Content -LiteralPath (Join-Path $root 'package.json') -Raw | ConvertFrom-Json).version
    $commit = (& git -C $root rev-parse HEAD | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Fixture needs the real source checkout.' }
    $installerName = "SAG-Wiki-$version-Windows-x64-setup.exe"
    $installerPath = Join-Path $inputDirectory $installerName
    foreach ($path in @($installerPath, (Join-Path $installation 'sag-desktop.exe'), (Join-Path $installation 'bin/sag.exe'))) {
        [IO.File]::WriteAllText($path, 'policy fixture only; never execute')
    }
    if ($IncludeUninstaller) { [IO.File]::WriteAllText((Join-Path $installation 'uninstall.exe'), 'policy fixture only; never execute') }
    $manifest = Copy-Metadata
    $manifest.version = $version
    $manifest.commit = $commit
    $manifest.build_identity.version = $version
    $manifest.files = [pscustomobject]@{$installerName = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()}
    $manifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $inputDirectory 'metadata.json') -Encoding utf8
    return [pscustomobject]@{
        input = $inputDirectory
        output = Join-Path $directory 'proof'
        manifest = $manifest
        smoke = [pscustomobject]@{
            status = 'passed'; version = $version; desktop_started = $true
            desktop = Join-Path $installation 'sag-desktop.exe'
            cli = Join-Path $installation 'bin/sag.exe'
            directory = Split-Path -Parent $installation
        }
    }
}
function Invoke-PolicyWorkflow($Fixture, [string]$SignatureStatus = 'Valid', [string]$RejectedLeaf = '') {
    # Keep real manifest checks, hashing and receipt publication. Replace only
    # Windows OS signature lookup, installation/desktop startup, and CLI E2E.
    $previous = @{}
    foreach ($name in @('OS', 'RUNNER_TEMP', 'GITHUB_SHA')) { $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    $originalSmoke = (Get-Item Function:Invoke-WindowsPackageSmoke).ScriptBlock
    $script:mockSignatureStatus = $SignatureStatus
    $script:mockRejectedLeaf = $RejectedLeaf
    $script:mockSmoke = $Fixture.smoke
    $script:smokeInvoked = $false
    try {
        $env:OS = 'Windows_NT'
        $env:RUNNER_TEMP = $temporary
        $env:GITHUB_SHA = $Fixture.manifest.commit
        function script:Get-AuthenticodeSignature {
            param([string]$LiteralPath)
            $signature = New-SignatureFixture
            if (-not $script:mockRejectedLeaf -or (Split-Path -Leaf $LiteralPath) -ceq $script:mockRejectedLeaf) {
                $signature.Status = $script:mockSignatureStatus
            }
            return $signature
        }
        function script:Invoke-WindowsPackageSmoke { $script:smokeInvoked = $true; return $script:mockSmoke }
        function script:python { $global:LASTEXITCODE = 0 }
        return Invoke-WindowsSigningVerification -InputDirectory $Fixture.input -OutputDirectory $Fixture.output -ExpectedPublisher 'SAG Release Publisher'
    } finally {
        Remove-Item Function:script:Get-AuthenticodeSignature, Function:script:python
        Set-Item Function:script:Invoke-WindowsPackageSmoke -Value $originalSmoke
        foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
    }
}

$signer = New-PolicyCertificate 'SAG Release Publisher' '1.3.6.1.5.5.7.3.3'
$timestamp = New-PolicyCertificate 'Timestamp Fixture' '1.3.6.1.5.5.7.3.8'
$wrongEku = New-PolicyCertificate 'SAG Release Publisher' '1.3.6.1.5.5.7.3.1'
$noEku = New-PolicyCertificate 'SAG Release Publisher' ''
$metadata = [pscustomobject]@{
    schema = 1
    version = '0.2.4'
    commit = 'a' * 40
    platform = 'windows'
    target = 'x86_64-pc-windows-msvc'
    build_identity = [pscustomobject]@{version = '0.2.4'; channel = 'release'}
    files = [pscustomobject]@{'SAG-Wiki-0.2.4-Windows-x64-setup.exe' = 'b' * 64}
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('sag-windows-signing-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    Test-Case 'accepts embedded Valid code signatures from the expected publisher' {
        $evidence = Assert-WindowsCodeSignature -Signature (New-SignatureFixture) -ExpectedPublisher 'sag release publisher'
        Assert-Equal $evidence.status 'Valid'
        Assert-Equal $evidence.subject 'CN=SAG Release Publisher'
        Assert-Equal $evidence.thumbprint $signer.Thumbprint.ToUpperInvariant()
        Assert-Equal $evidence.timestamp_thumbprint $timestamp.Thumbprint.ToUpperInvariant()
    }
    Test-Case 'rejects a different publisher' {
        Assert-Throws { Assert-WindowsCodeSignature -Signature (New-SignatureFixture) -ExpectedPublisher 'Another Publisher' } 'publisher'
    }
    Test-Case 'requires an explicitly configured publisher' {
        Assert-Throws { Assert-WindowsCodeSignature -Signature (New-SignatureFixture) -ExpectedPublisher '' } 'WINDOWS_SIGNING_PUBLISHER'
    }
    foreach ($status in @('NotSigned', 'HashMismatch', 'NotTrusted', 'UnknownError')) {
        Test-Case "rejects OS signature status $status" {
            $signature = New-SignatureFixture
            $signature.Status = $status
            Assert-Throws { Assert-WindowsCodeSignature -Signature $signature -ExpectedPublisher 'SAG Release Publisher' } $status
        }
    }
    Test-Case 'rejects a catalog signature even when its status is Valid' {
        $signature = New-SignatureFixture
        $signature.SignatureType = 'Catalog'
        Assert-Throws { Assert-WindowsCodeSignature -Signature $signature -ExpectedPublisher 'SAG Release Publisher' } 'embedded Authenticode'
    }
    Test-Case 'requires the signer certificate' {
        $signature = New-SignatureFixture
        $signature.SignerCertificate = $null
        Assert-Throws { Assert-WindowsCodeSignature -Signature $signature -ExpectedPublisher 'SAG Release Publisher' } 'signer certificate'
    }
    Test-Case 'requires a trusted timestamp on a Valid signature' {
        $signature = New-SignatureFixture
        $signature.TimeStamperCertificate = $null
        Assert-Throws { Assert-WindowsCodeSignature -Signature $signature -ExpectedPublisher 'SAG Release Publisher' } 'timestamp'
    }
    foreach ($certificate in @($wrongEku, $noEku)) {
        Test-Case 'rejects a certificate without the code-signing EKU' {
            $signature = New-SignatureFixture
            $signature.SignerCertificate = $certificate
            Assert-Throws { Assert-WindowsCodeSignature -Signature $signature -ExpectedPublisher 'SAG Release Publisher' } 'code-signing EKU'
        }
    }
    Test-Case 'accepts the exact raw Windows release manifest' {
        Assert-Equal (Assert-Metadata $metadata) 'SAG-Wiki-0.2.4-Windows-x64-setup.exe'
    }
    foreach ($mutation in @(
        @{field = 'schema'; value = 2; rejection = 'schema'},
        @{field = 'version'; value = '0.2.5'; rejection = 'version'},
        @{field = 'commit'; value = ('c' * 40); rejection = 'commit'},
        @{field = 'platform'; value = 'macos'; rejection = 'platform'},
        @{field = 'target'; value = 'aarch64-pc-windows-msvc'; rejection = 'target'}
    )) {
        Test-Case "rejects wrong metadata $($mutation.field)" {
            $changed = Copy-Metadata
            $changed.($mutation.field) = $mutation.value
            Assert-Throws { Assert-Metadata $changed } $mutation.rejection
        }
    }
    Test-Case 'binds checkout to the triggering commit' {
        Assert-Throws { Assert-WindowsReleaseMetadata -Metadata $metadata -Version '0.2.4' -Commit ('a' * 40) -TriggerCommit ('c' * 40) -FileNames @('metadata.json', 'SAG-Wiki-0.2.4-Windows-x64-setup.exe') } 'triggering commit'
    }
    Test-Case 'rejects a missing metadata field' {
        $changed = Copy-Metadata
        $changed.PSObject.Properties.Remove('commit')
        Assert-Throws { Assert-Metadata $changed } 'commit'
    }
    Test-Case 'rejects development CLI metadata' {
        $changed = Copy-Metadata
        $changed.build_identity.channel = 'dev'
        Assert-Throws { Assert-Metadata $changed } 'release CLI'
    }
    Test-Case 'rejects a mismatched CLI version' {
        $changed = Copy-Metadata
        $changed.build_identity.version = '0.2.5'
        Assert-Throws { Assert-Metadata $changed } 'CLI version'
    }
    Test-Case 'rejects additional or renamed artifact names' {
        $changed = Copy-Metadata
        $changed.files = [pscustomobject]@{'../installer.exe' = 'b' * 64}
        Assert-Throws { Assert-Metadata $changed } 'artifact file list'
    }
    Test-Case 'rejects untracked files in raw staging' {
        Assert-Throws { Assert-Metadata $metadata @('metadata.json', 'SAG-Wiki-0.2.4-Windows-x64-setup.exe', 'extra.exe') } 'staging file list'
    }
    Test-Case 'rejects malformed SHA-256 values' {
        $changed = Copy-Metadata
        $changed.files.'SAG-Wiki-0.2.4-Windows-x64-setup.exe' = 'not-a-sha256'
        Assert-Throws { Assert-Metadata $changed } 'SHA-256'
    }
    Test-Case 'rejects a raw digest mismatch before signature lookup' {
        $artifact = Join-Path $temporary 'digest-test.exe'
        [IO.File]::WriteAllText($artifact, 'not an installer')
        Assert-Throws { Assert-WindowsArtifactDigest -Path $artifact -ExpectedSha256 ('0' * 64) } 'checksum mismatch'
    }
    Test-Case 'parses the final successful desktop smoke JSON after diagnostic output' {
        $output = "Diagnostic line`n" + (@{status = 'passed'; version = '0.2.4'; desktop_started = $true; desktop = 'desktop.exe'; cli = 'sag.exe'; directory = 'smoke'} | ConvertTo-Json)
        $result = ConvertFrom-WindowsSmokeOutput -Output $output -Version '0.2.4'
        Assert-Equal $result.cli 'sag.exe'
    }
    Test-Case 'rejects smoke evidence that skipped the desktop' {
        $output = @{status = 'passed'; version = '0.2.4'; desktop_started = $false; desktop = 'desktop.exe'; cli = 'sag.exe'; directory = 'smoke'} | ConvertTo-Json
        Assert-Throws { ConvertFrom-WindowsSmokeOutput -Output $output -Version '0.2.4' } 'desktop'
    }
    Test-Case 'rejects non-JSON smoke output' {
        Assert-Throws { ConvertFrom-WindowsSmokeOutput -Output 'failed to start' -Version '0.2.4' } 'JSON'
    }
    Test-Case 'unsigned input never invokes the installer or publishes a receipt' {
        $fixture = New-WorkflowFixture 'unsigned'
        Assert-Throws { Invoke-PolicyWorkflow $fixture 'NotSigned' } 'NotSigned'
        Assert-Equal $script:smokeInvoked $false
        Assert-Equal (Test-Path -LiteralPath $fixture.output) $false
    }
    Test-Case 'missing installed uninstaller prevents publication' {
        $fixture = New-WorkflowFixture 'missing-uninstaller' $false
        Assert-Throws { Invoke-PolicyWorkflow $fixture } 'uninstall'
        Assert-Equal (Test-Path -LiteralPath $fixture.output) $false
    }
    foreach ($leaf in @('sag-desktop.exe', 'sag.exe', 'uninstall.exe')) {
        Test-Case "untrusted installed $leaf prevents publication" {
            $fixture = New-WorkflowFixture ('untrusted-' + $leaf)
            Assert-Throws { Invoke-PolicyWorkflow $fixture 'NotTrusted' $leaf } 'NotTrusted'
            Assert-Equal (Test-Path -LiteralPath $fixture.output) $false
        }
    }
    Test-Case 'publishes exactly one receipt covering all four verified executable roles' {
        $fixture = New-WorkflowFixture 'accepted-policy'
        $receipt = Invoke-PolicyWorkflow $fixture
        $files = @(Get-ChildItem -LiteralPath $fixture.output -Force)
        Assert-Equal $files.Count 1
        Assert-Equal $files[0].Name 'windows-signing.json'
        $saved = Get-Content -LiteralPath $files[0].FullName -Raw | ConvertFrom-Json
        Assert-Equal $saved.schema 1
        Assert-Equal $saved.commit $fixture.manifest.commit
        Assert-Equal $saved.version $fixture.manifest.version
        Assert-Equal $saved.windows_signing.publisher 'SAG Release Publisher'
        Assert-Equal @($saved.windows_signing.files.PSObject.Properties).Count 4
        foreach ($role in @('installer', 'desktop', 'cli', 'uninstaller')) {
            Assert-Equal $saved.windows_signing.files.$role.status 'Valid'
            Assert-Equal $saved.windows_signing.files.$role.subject 'CN=SAG Release Publisher'
        }
        $installerName = $fixture.manifest.files.PSObject.Properties.Name
        Assert-Equal $saved.files.$installerName $fixture.manifest.files.$installerName
        Assert-Equal $saved.windows_signing.files.installer.sha256 $fixture.manifest.files.$installerName
        Assert-Throws { Invoke-PolicyWorkflow $fixture } 'fresh'
        Assert-Equal (Get-Content -LiteralPath $files[0].FullName -Raw) (($receipt | ConvertTo-Json -Depth 20) + "`n")
    }
} finally {
    foreach ($certificate in @($signer, $timestamp, $wrongEku, $noEku)) { $certificate.Dispose() }
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
Write-Host "$script:passed passed; $script:failed failed. Policy fixtures do not establish Windows public-CA trust."
if ($script:failed -gt 0) { exit 1 }
