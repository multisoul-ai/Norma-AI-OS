param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [ValidateSet('window-close','quit-shortcut','settings-then-quit','configured-input-quit')][string]$ExitMode='window-close'
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/desktop-driver.ps1"
New-Item -ItemType Directory -Force $Output | Out-Null
$Output=(Resolve-Path $Output).Path
$exe=(Resolve-Path $Executable).Path
$process=$null
$configured=$false
$database=Join-Path $env:USERPROFILE 'Library/Application Support/Norma OS/app.sqlite'
$backup=Join-Path $Output 'shortcut-before.json'
$report=@{status='FAIL'; source_sha=$env:GITHUB_SHA; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; exit_mode=$ExitMode; scope='Actual development App startup, native foreground, real WebView2 creation, rendered frames and native exit input; no package acceptance'}
try {
    if ($ExitMode -eq 'configured-input-quit') {
        python "$PSScriptRoot/shortcut-fixture.py" configure --database $database --backup $backup
        if ($LASTEXITCODE -ne 0) { throw 'Could not configure the persisted Quit fixture' }
        $configured=$true
        $report.configured_quit='ctrl-a'
    }
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
        if ($ExitMode -eq 'configured-input-quit') {
            Send-NormaChord @(0x11,0x4B) # Ctrl+K focuses the real command InputState.
            Start-Sleep -Milliseconds 300
            [NormaDesktop]::Text('native quit focus probe')
            Start-Sleep -Milliseconds 300
            Save-NormaScreenshot $window (Join-Path $Output 'focused-input-before-quit.png')
            Send-NormaChord @(0x11,0x10,0x51)
            if ($process.WaitForExit(1000)) { throw 'Overridden default Quit must not exit App' }
            $report.overridden_default_ignored=$true
            Send-NormaChord @(0x11,0x41) # Configured Ctrl+A must precede InputState SelectAll.
        } else {
            Send-NormaChord @(0x11,0x10,0x51) # Ctrl+Shift+Q must reach the App-owned quit chain.
        }
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
    $restoreFailed=$false
    if ($process) { $process.Refresh(); if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit(10000) | Out-Null }; $process.Dispose() }
    if ($configured) {
        python "$PSScriptRoot/shortcut-fixture.py" restore --database $database --backup $backup
        if ($LASTEXITCODE -ne 0) {
            $restoreFailed=$true
            $report.status='FAIL'
            $report.error='Could not restore the original runner shortcut'
        } else { $report.shortcut_fixture_restored=$true }
    }
    $logs=Join-Path $env:USERPROFILE '.norma/logs'
    if (Test-Path $logs) { Copy-Item $logs (Join-Path $Output 'app-logs') -Recurse }
    $report | ConvertTo-Json | Set-Content (Join-Path $Output 'report.json') -Encoding utf8
    if ($restoreFailed) { throw $report.error }
}
