param([Parameter(Mandatory)][string]$Executable, [Parameter(Mandatory)][string]$Output)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/desktop-driver.ps1"
New-Item -ItemType Directory -Force $Output | Out-Null
$Output=(Resolve-Path $Output).Path
$exe=(Resolve-Path $Executable).Path
$process=$null
$report=@{status='FAIL'; source_sha=$env:GITHUB_SHA; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; scope='Actual development App startup, native foreground, real WebView2 creation, three rendered frames and WM_CLOSE; no package acceptance'}
try {
    $process=Start-Process $exe -WorkingDirectory (Split-Path $exe) -PassThru -RedirectStandardOutput (Join-Path $Output 'stdout.log') -RedirectStandardError (Join-Path $Output 'stderr.log')
    $window=Get-NormaWindow $process
    Focus-NormaWindow $window
    $readyDeadline=[DateTime]::UtcNow.AddSeconds(45)
    while (-not (Select-String -Path (Join-Path $Output 'stdout.log') -Pattern 'native_webview_ready' -Quiet)) {
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
    Close-NormaWindow $process $window
    $report.status='PASS'
} catch { $report.error=$_.Exception.Message; throw }
finally {
    $report | ConvertTo-Json | Set-Content (Join-Path $Output 'report.json') -Encoding utf8
    if ($process) { $process.Refresh(); if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit(10000) | Out-Null }; $process.Dispose() }
    $logs=Join-Path $env:USERPROFILE '.norma/logs'
    if (Test-Path $logs) { Copy-Item $logs (Join-Path $Output 'app-logs') -Recurse }
}
