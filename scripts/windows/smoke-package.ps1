param([Parameter(Mandatory)][string]$Package, [Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NormaWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr first, IntPtr second);
}
'@
New-Item -ItemType Directory -Force $Output | Out-Null
$process = $null
try {
$root = (Resolve-Path $Package).Path
python scripts/windows/package.py --verify $root
if ($LASTEXITCODE -ne 0) { throw 'Candidate integrity validation failed' }
$version = & (Join-Path $root 'norma.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'Packaged CLI did not execute' }
$hook = Start-Process -FilePath (Join-Path $root 'multisoul-agent-hook.exe') -ArgumentList '--help' -WorkingDirectory $root -PassThru -RedirectStandardOutput (Join-Path $Output 'hook-help.log') -RedirectStandardError (Join-Path $Output 'hook-stderr.log')
if (-not $hook.WaitForExit(10000)) { $hook.Kill($true); throw 'Packaged hook helper did not exit within 10 seconds' }
if ($hook.ExitCode -ne 0 -or -not (Select-String -Path (Join-Path $Output 'hook-help.log') -Pattern 'forward' -Quiet)) { throw 'Packaged hook helper did not execute its real CLI' }
$browser = Start-Process -FilePath (Join-Path $root 'norma-browser-auth-host.exe') -ArgumentList 'unauthorized-probe-origin' -WorkingDirectory $root -PassThru -RedirectStandardOutput (Join-Path $Output 'browser-host-stdout.log') -RedirectStandardError (Join-Path $Output 'browser-host-stderr.log')
if (-not $browser.WaitForExit(10000)) { $browser.Kill($true); throw 'Packaged browser helper did not exit within 10 seconds' }
if ($browser.ExitCode -ne 1 -or -not (Select-String -Path (Join-Path $Output 'browser-host-stderr.log') -Pattern 'caller was not the authorized Norma Chrome extension' -Quiet)) { throw 'Packaged browser helper did not enforce its caller boundary' }
$manifest = Get-Content (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json
if ($manifest.source_sha -ne $env:GITHUB_SHA) { throw 'Candidate source identity does not match this workflow' }
    $process = Start-Process -FilePath (Join-Path $root 'norma-os.exe') -WorkingDirectory $root -PassThru -RedirectStandardOutput (Join-Path $Output 'stdout.log') -RedirectStandardError (Join-Path $Output 'stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(45)
    $window = [IntPtr]::Zero
    while ([DateTime]::UtcNow -lt $deadline) {
        $process.Refresh()
        if ($process.HasExited) { throw "Packaged App exited before opening a window: $($process.ExitCode)" }
        $window = $process.MainWindowHandle
        if ($window -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($window -eq [IntPtr]::Zero) { throw 'Packaged App did not open a visible window' }
    Start-Sleep -Seconds 3
    $rect = [NormaWindow+Rect]::new()
    if (-not [NormaWindow]::GetWindowRect($window, [ref]$rect)) { throw 'Candidate window bounds are unavailable' }
    $width = $rect.Right - $rect.Left; $height = $rect.Bottom - $rect.Top
    if ($width -lt 300 -or $height -lt 200) { throw 'Candidate window dimensions are invalid' }
    $bitmap = [System.Drawing.Bitmap]::new($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $Output 'candidate-window.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    if (-not [NormaWindow]::PostMessage($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Unable to close the candidate window' }
    if (-not $process.WaitForExit(10000)) { throw 'Candidate remained running after closing its last window' }
    if ($process.ExitCode -ne 0) { throw "Candidate exited with $($process.ExitCode)" }
    @{ status = 'PASS'; source_sha = $manifest.source_sha; run_id = $env:GITHUB_RUN_ID; run_attempt = $env:GITHUB_RUN_ATTEMPT; version = $version; width = $width; height = $height; exit_code = $process.ExitCode; scope = 'Packaged CLI, hook help, browser caller rejection, window and normal last-window close; no model task' } | ConvertTo-Json | Set-Content (Join-Path $Output 'report.json')
} catch {
    @{ status = 'FAIL'; source_sha = $env:GITHUB_SHA; run_id = $env:GITHUB_RUN_ID; run_attempt = $env:GITHUB_RUN_ATTEMPT; error = $_.Exception.Message } | ConvertTo-Json | Set-Content (Join-Path $Output 'report.json')
    throw
} finally {
    if ($process) { $process.Refresh(); if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit(10000) | Out-Null }; $process.Dispose() }
}
