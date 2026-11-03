param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [ValidateSet('window-close','quit-shortcut','settings-then-quit','configured-input-quit','workspace-reopen')][string]$ExitMode='window-close',
    [string]$SourceSha=$env:GITHUB_SHA
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
$report=@{status='FAIL'; source_sha=$SourceSha; probe_sha=$env:GITHUB_SHA; executable=$exe; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; exit_mode=$ExitMode; scope='Actual App startup, native foreground, real WebView2 creation, rendered frames and native exit input; package provenance is checked separately by the caller'}
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
    if ($ExitMode -eq 'workspace-reopen') {
        $report.scope='Actual App: native Ctrl+N, Unicode workspace creation, close and reopen; screenshots require visual verification; package provenance is checked separately by the caller'
        $name="Runner 中文 workspace $env:GITHUB_RUN_ID"
        Send-NormaChord @(0x11,0x4E)
        Start-Sleep -Seconds 1
        Save-NormaScreenshot $window (Join-Path $Output 'workspace-create-dialog.png')
        [NormaDesktop]::Text($name)
        Start-Sleep -Milliseconds 300
        Save-NormaScreenshot $window (Join-Path $Output 'workspace-name-entered.png')
        Send-NormaChord @(0x0D)
        Start-Sleep -Milliseconds 300
        Save-NormaScreenshot $window (Join-Path $Output 'workspace-name-submitted.png')
        python "$PSScriptRoot/read-workspace.py" --database $database --name $name --output (Join-Path $Output 'workspace-created.json') --wait-seconds 10
        if ($LASTEXITCODE -ne 0) { throw 'Native workspace creation did not persist the exact Unicode name' }
        Start-Sleep -Milliseconds 300
        Save-NormaScreenshot $window (Join-Path $Output 'workspace-created.png')
        Close-NormaWindow $process $window
        $process.Dispose(); $process=$null
        $reopened=Join-Path $Output 'reopened'
        New-Item -ItemType Directory -Force $reopened | Out-Null
        $process=Start-Process $exe -WorkingDirectory (Split-Path $exe) -PassThru -RedirectStandardOutput (Join-Path $reopened 'stdout.log') -RedirectStandardError (Join-Path $reopened 'stderr.log')
        $window=Get-NormaWindow $process
        Focus-NormaWindow $window
        $readyDeadline=[DateTime]::UtcNow.AddSeconds(45)
        while (-not (Test-NormaNativeReady $reopened)) {
            if ($process.WaitForExit(0)) { throw 'App exited before reopening the persisted workspace' }
            if ([DateTime]::UtcNow -ge $readyDeadline) { throw 'Reopened App did not finish native browser creation' }
            Start-Sleep -Milliseconds 250
        }
        Start-Sleep -Seconds 1
        Save-NormaScreenshot $window (Join-Path $Output 'workspace-reopened.png')
        python "$PSScriptRoot/read-workspace.py" --database $database --name $name --output (Join-Path $Output 'workspace-reopened.json')
        if ($LASTEXITCODE -ne 0) { throw 'Created workspace did not survive close and reopen' }
        $createdRows=@(Get-Content (Join-Path $Output 'workspace-created.json') -Raw | ConvertFrom-Json)
        $reopenedRows=@(Get-Content (Join-Path $Output 'workspace-reopened.json') -Raw | ConvertFrom-Json)
        if ($createdRows[0].id -ne $reopenedRows[0].id) { throw 'Reopen must preserve the exact workspace identity' }
        Close-NormaWindow $process $window
        $report.workspace_name=$name
        $report.workspace_id=$createdRows[0].id
        $report.workspace_persisted=$true
        $report.visual_verification='PENDING'
    } elseif ($ExitMode -eq 'window-close') {
        Close-NormaWindow $process $window
    } else {
        if ($ExitMode -eq 'settings-then-quit') {
            Send-NormaChord @(0x11,0xBC) # Ctrl+, requests the actual Settings panel.
            Start-Sleep -Seconds 1
            Save-NormaScreenshot $window (Join-Path $Output 'settings-before-quit.png')
        }
        if ($ExitMode -eq 'configured-input-quit') {
            Send-NormaChord @(0x11,0x4B) # Ctrl+K focuses the real command InputState.
            $focusDeadline=[DateTime]::UtcNow.AddSeconds(5)
            while ($true) {
                $focusLog=(@('stdout.log','stderr.log') | ForEach-Object { Get-Content (Join-Path $Output $_) -Raw }) -join "`n"
                $focusLog=$focusLog -replace '\x1b\[[0-9;]*m',''
                if ($focusLog -match 'stage="focus_applied"[^\r\n]*\bfocused=true\b') { break }
                if ($process.WaitForExit(0)) { throw 'App exited before the command input gained focus' }
                if ([DateTime]::UtcNow -ge $focusDeadline) { throw 'Real command InputState did not gain focus' }
                Start-Sleep -Milliseconds 100
            }
            $report.command_input_focused=$true
            [NormaDesktop]::Text('native quit focus probe')
            Start-Sleep -Milliseconds 300
            Save-NormaScreenshot $window (Join-Path $Output 'focused-input-before-quit.png')
            Send-NormaChord @(0x11,0x10,0x51)
            # Use the full positive Quit deadline so a delayed old-key exit cannot pass as Ctrl+A.
            if ($process.WaitForExit(10000)) { throw 'Overridden default Quit must not exit App' }
            $report.overridden_default_ignored=$true
            Send-NormaChord @(0x11,0x41) # Configured Ctrl+A must precede InputState SelectAll.
        } else {
            Send-NormaChord @(0x11,0x10,0x51) # Ctrl+Shift+Q must reach the App-owned quit chain.
        }
        if (-not $process.WaitForExit(10000)) { throw 'Native quit shortcut did not exit App' }
        if ($process.ExitCode -ne 0) { throw "Native quit shortcut exit code $($process.ExitCode)" }
    }
    $streams=@('stdout.log','stderr.log')
    if ($ExitMode -eq 'workspace-reopen') { $streams += @('reopened/stdout.log','reopened/stderr.log') }
    foreach ($stream in $streams) {
        $content=(Get-Content (Join-Path $Output $stream) -Raw) -replace '\x1b\[[0-9;]*m', ''
        if ($content -match '(?m)^\S+\s+ERROR\b|^\[[^\r\n]*:ERROR:|panicked at') { throw "Actual App emitted an error during startup/close; inspect $stream" }
    }
    $report.status='PASS'
} catch { $report.error=$_.Exception.Message; throw }
finally {
    $cleanupErrors=@()
    $processStopped=($null -eq $process)
    try {
        if ($process) {
            if (-not $process.WaitForExit(0)) {
                try { $process.Kill($true) }
                catch { if (-not $process.WaitForExit(0)) { throw } }
                if (-not $process.WaitForExit(10000)) { throw 'App remained alive after cleanup' }
            }
            $processStopped=$true
        }
    } catch { $cleanupErrors += $_.Exception.Message }
    try { if ($process) { $process.Dispose() } }
    catch { $cleanupErrors += $_.Exception.Message }
    if ($configured -and $processStopped) {
        try {
            python "$PSScriptRoot/shortcut-fixture.py" restore --database $database --backup $backup
            if ($LASTEXITCODE -ne 0) { throw 'Could not restore the original runner shortcut' }
            $report.shortcut_fixture_restored=$true
        } catch { $cleanupErrors += $_.Exception.Message }
    } elseif ($configured) {
        $cleanupErrors += 'App is still alive; its settings cannot be restored safely'
    }
    $logs=Join-Path $env:USERPROFILE '.norma/logs'
    try { if (Test-Path $logs) { Copy-Item $logs (Join-Path $Output 'app-logs') -Recurse } }
    catch { $cleanupErrors += $_.Exception.Message }
    if ($cleanupErrors.Count -gt 0) { $report.status='FAIL'; $report.cleanup_errors=$cleanupErrors }
    $report | ConvertTo-Json | Set-Content (Join-Path $Output 'report.json') -Encoding utf8
    if ($cleanupErrors.Count -gt 0) { throw ($cleanupErrors -join '; ') }
}
