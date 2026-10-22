# Run on Windows while the packaged SAG Wiki desktop app is open.
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class SagWindowIcon {
    [DllImport("user32.dll")]
    public static extern IntPtr SendMessageTimeoutW(
        IntPtr hwnd, uint message, UIntPtr wparam, IntPtr lparam,
        uint flags, uint timeout, out UIntPtr result);
}
'@
$app = Get-Process -Name 'sag-desktop' | Where-Object { $_.MainWindowHandle -ne 0 }
if (@($app).Count -ne 1) { throw 'Open exactly one packaged SAG Wiki desktop window.' }
[UIntPtr]$icon = [UIntPtr]::Zero
# WM_GETICON / ICON_BIG: query the actual running window, not the EXE or shortcut.
$sent = [SagWindowIcon]::SendMessageTimeoutW(
    $app.MainWindowHandle, 0x007F, [UIntPtr]1, [IntPtr]::Zero, 2, 5000, [ref]$icon)
if ($sent -eq [IntPtr]::Zero) { throw 'Could not query the window icon.' }
if ($icon -eq [UIntPtr]::Zero) { throw 'SAG Wiki has no taskbar icon (ICON_BIG).' }
Write-Output 'PASS: the running SAG Wiki window has a taskbar icon.'
