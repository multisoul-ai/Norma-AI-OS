param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [ValidateSet('window-close','quit-shortcut','settings-then-quit')][string]$ExitMode='window-close'
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/desktop-driver.ps1"
New-Item -ItemType Directory -Force $Output | Out-Null
$Output=(Resolve-Path $Output).Path
$exe=(Resolve-Path $Executable).Path
$process=$null
$report=@{status='FAIL'; source_sha=$env:GITHUB_SHA; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; exit_mode=$ExitMode; scope='Actual development App startup, native foreground, real WebView2 creation, rendered frames and native exit input; no package acceptance'}
try {
    $process=Start-Process $exe -WorkingDirectory (Split-Path $exe) -PassThru -RedirectStandardOutput (Join-Path $Output 'stdout.log') -RedirectStandardError (Join-Path $Output 'stderr.log')
    $window=Get-NormaWindow $process
    Focus-NormaWindow $window
    $readyDeadline=[DateTime]::UtcNow.AddSeconds(45)
    while (-not (Test-NormaNativeReady $Output)) {
        $process.Refresh()
        if ($process.HasExited) { throw "Actual App exited during native browser creation: $($process.ExitCode)" }
        if ([DateTime]::UtcNow -ge $readyDeadline) { throw 'WebView2 did not finish real native creation' }
        Start-Sleep -Milliseconds 250
    }
    foreach ($frame in 1..3) {
        Start-Sleep -Seconds 1
        $process.Refresh()
        if ($process.HasExited) { throw "Actual App exited before stable rendering: $($process.ExitCode)" }
        Save-NormaScreenshot $window (Join-Path $Output "frame-$frame.png")
    }
    if ($ExitMode -eq 'window-close') {
        Close-NormaWindow $process $window
    } else {
        if ($ExitMode -eq 'settings-then-quit') {
            Send-NormaChord @(0x11,0xBC) # Ctrl+, requests the actual Settings panel.
            Start-Sleep -Seconds 1
            Save-NormaScreenshot $window (Join-Path $Output 'settings-before-quit.png')
        }
        Send-NormaChord @(0x11,0x10,0x51) # Ctrl+Shift+Q must reach the App-owned quit chain.
        if (-not $process.WaitForExit(10000)) { throw 'Native quit shortcut did not exit App' }
        if ($process.ExitCode -ne 0) { throw "Native quit shortcut exit code $($process.ExitCode)" }
    }
    foreach ($stream in @('stdout.log', 'stderr.log')) {
        $content=(Get-Content (Join-Path $Output $stream) -Raw) -replace '\x1b\[[0-9;]*m', ''
        if ($content -match '(?m)^\S+\s+ERROR\b|panicked at') { throw "Actual App emitted an error during startup/close; inspect $stream" }
    }
    $report.status='PASS'
} catch { $report.error=$_.Exception.Message; throw }
finally {
    $report | ConvertTo-Json | Set-Content (Join-Path $Output 'report.json') -Encoding utf8
    if ($process) { $process.Refresh(); if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit(10000) | Out-Null }; $process.Dispose() }
    $logs=Join-Path $env:USERPROFILE '.norma/logs'
    if (Test-Path $logs) { Copy-Item $logs (Join-Path $Output 'app-logs') -Recurse }
}
