param(
    [string]$TestExecutable,
    [string]$Root,
    [string]$Python,
    [switch]$Worker
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$env:PYTHONUTF8 = '1'
$testName = 'docling::tests::real_windows_unicode_profile_installs_and_reads_scanned_pdf'

function Invoke-DoclingAcceptance([string]$Phase) {
    $env:SAG_TEST_WINDOWS_DOCLING_PHASE = $Phase
    $log = Join-Path $Root "diagnostics/$Phase-test.log"
    & (Join-Path $Root 'docling-test.exe') --exact $testName --ignored --nocapture *> $log
    $code = $LASTEXITCODE
    $installLog = Join-Path $env:APPDATA "sag-docling-acceptance/$Phase/docling/install-diagnostics.log"
    if (Test-Path -LiteralPath $installLog) {
        # Retain progress and parser stack lines. Download logs can include signed URLs.
        Get-Content -LiteralPath $installLog |
            Where-Object { $_ -match '^(Offline probe|Installed |ERROR:docling|WARNING:docling|Traceback|  File |RuntimeError:)' } |
            ForEach-Object { $_ -replace 'https?://\S+', '[URL removed]' } |
            Out-File -LiteralPath (Join-Path $Root "diagnostics/$Phase-install.log") -Encoding utf8
    }
    # An incorrect filter also exits successfully with zero tests.
    if ($code -ne 0 -or -not (Select-String -LiteralPath $log -SimpleMatch '1 passed')) {
        throw "$Phase Docling acceptance did not pass. See $log"
    }
}

if ($Worker) {
    try {
        Start-Transcript -Path (Join-Path $Root 'diagnostics/profile.log') | Out-Null
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
        $ansi = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Nls\CodePage').ACP
        Write-Host "Account: $identity, profile: $env:USERPROFILE, AppData: $env:APPDATA, ANSI code page: $ansi"
        if ($identity.Split('\')[-1] -cne $env:USERNAME -or $env:USERNAME -notmatch '[^\x00-\x7F]' -or $ansi -eq '65001') {
            throw 'Expected a real non-ASCII local user with a non-UTF-8 system ANSI code page.'
        }
        Invoke-DoclingAcceptance 'online'

        $installed = Join-Path $env:APPDATA 'sag-docling-acceptance/online/docling'
        $active = Get-Content -Raw -LiteralPath (Join-Path $installed 'active.json') | ConvertFrom-Json
        $component = Join-Path $installed $active.directory
        $manifest = Get-Content -Raw -LiteralPath (Join-Path $component 'manifest.json') | ConvertFrom-Json
        $bundle = Join-Path $Root "Docling-$($manifest.component_version)-windows-x86_64"
        $packager = Join-Path $Root 'package-docling-component.py'
        & $Python $packager --platform windows-x86_64 --python-runtime (Split-Path (Join-Path $component $manifest.python)) `
            --site-packages (Join-Path $component $manifest.site_packages) --models (Join-Path $component $manifest.models) `
            --requirements-lock (Join-Path $component 'requirements.txt') --output $bundle
        if ($LASTEXITCODE -ne 0) { throw 'Offline component packaging failed.' }

        $zip = "$bundle.zip"
        & $Python -m zipfile -c $zip $bundle
        if ($LASTEXITCODE -ne 0) { throw 'Offline component ZIP creation failed.' }
        $unpacked = Join-Path $Root 'unpacked'
        & $Python -m zipfile -e $zip $unpacked
        if ($LASTEXITCODE -ne 0) { throw 'Offline component ZIP extraction failed.' }
        $env:SAG_TEST_WINDOWS_DOCLING_PACKAGE = Join-Path $unpacked (Split-Path -Leaf $bundle)
        & $Python $packager --verify $env:SAG_TEST_WINDOWS_DOCLING_PACKAGE
        if ($LASTEXITCODE -ne 0) { throw 'Extracted offline component failed checksum validation.' }

        $env:HF_ENDPOINT = 'http://127.0.0.1:1'
        $env:HF_HUB_OFFLINE = '1'
        $env:TRANSFORMERS_OFFLINE = '1'
        $env:UV_OFFLINE = '1'
        Invoke-DoclingAcceptance 'offline'
        Write-Host "Validated both installations on $identity."
        Stop-Transcript | Out-Null
        exit 0
    } catch {
        Write-Host $_
        Stop-Transcript -ErrorAction SilentlyContinue | Out-Null
        exit 1
    }
}

$id = [guid]::NewGuid().ToString('N').Substring(0, 8)
$Root = Join-Path "$env:SystemDrive\" "sag-docling-$id"
$Python = (Get-Command python.exe).Source
$username = '扫描测试' + $id
$password = 'T9!a' + [guid]::NewGuid().ToString('N')
$taskName = "sag-docling-acceptance-$id"
$output = Join-Path $PWD 'qa/windows-docling-acceptance'
New-Item -ItemType Directory -Path (Join-Path $Root 'diagnostics'), $output -Force | Out-Null

try {
    $account = New-LocalUser -Name $username -Password (ConvertTo-SecureString $password -AsPlainText -Force)
    $users = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')
    if ($account.SID.Value -notin (Get-LocalGroupMember -SID $users).SID.Value) {
        Add-LocalGroupMember -SID $users -Member $account
    }
    & icacls $Root /grant "*$($account.SID.Value):(OI)(CI)M" /q
    if ($LASTEXITCODE -ne 0) { throw 'Could not grant the test account access to the fixture workspace.' }
    Copy-Item -LiteralPath $TestExecutable -Destination (Join-Path $Root 'docling-test.exe')
    Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $Root 'worker.ps1')
    Copy-Item -LiteralPath 'scripts/package-docling-component.py' -Destination $Root

    $task = New-ScheduledTaskAction -Execute (Get-Command pwsh.exe).Source `
        -Argument "-NoLogo -NoProfile -File `"$Root\worker.ps1`" -Worker -Root `"$Root`" -Python `"$Python`""
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 45)
    Register-ScheduledTask -TaskName $taskName -Action $task -Settings $settings `
        -User "$env:COMPUTERNAME\$username" -Password $password -RunLevel Limited | Out-Null
    $previousRun = (Get-ScheduledTaskInfo -TaskName $taskName).LastRunTime
    Start-ScheduledTask -TaskName $taskName
    $deadline = (Get-Date).AddMinutes(45)
    do {
        Start-Sleep -Seconds 5
        $info = Get-ScheduledTaskInfo -TaskName $taskName
        $state = (Get-ScheduledTask -TaskName $taskName).State
        if ((Get-Date) -ge $deadline) { throw 'Windows Docling acceptance timed out.' }
    } until ($info.LastRunTime -gt $previousRun -and $state -notin @('Queued', 'Running'))

    if ($info.LastTaskResult -ne 0) {
        throw ('Windows Docling acceptance exited with Task Scheduler result 0x{0:X}.' -f $info.LastTaskResult)
    }
    Get-ChildItem -Path $Root -File -Filter '*.zip' |
        Move-Item -Destination $output
} finally {
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
        Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
    Get-ChildItem -Path (Join-Path $Root 'diagnostics') -File | ForEach-Object {
        $content = Get-Content -LiteralPath $_.FullName -Raw
        if ($content) {
            $content = $content -replace 'https?://\S+', '[URL removed]'
            $content | Out-File -LiteralPath (Join-Path $output $_.Name) -Encoding utf8
            Write-Host $content
        }
    }
    Remove-LocalUser -Name $username -ErrorAction SilentlyContinue
}
