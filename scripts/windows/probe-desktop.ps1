param([Parameter(Mandatory)][string]$Package, [Parameter(Mandatory)][string]$Output, [Parameter(Mandatory)][string]$ExpectedSource)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/desktop-driver.ps1"
New-Item -ItemType Directory -Force $Output | Out-Null
$Output=(Resolve-Path $Output).Path
$root=(Resolve-Path $Package).Path
$manifest=Get-Content (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json
$process=$null
$report=@{ status='FAIL'; source_sha=$manifest.source_sha; probe_sha=$env:GITHUB_SHA; producer_run=$env:CANDIDATE_RUN; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; zip_sha256=$env:CANDIDATE_ZIP_SHA256; scope='Actual packaged App: Ctrl+N, Unicode workspace input, normal close and reopen; no model task' }
try {
    if ($manifest.source_sha -ne $ExpectedSource) { throw 'Candidate source identity mismatch' }
    python scripts/windows/package.py --verify $root
    if ($LASTEXITCODE -ne 0) { throw 'Candidate integrity validation failed' }
    $process=Start-Process (Join-Path $root 'norma-os.exe') -WorkingDirectory $root -PassThru -RedirectStandardOutput (Join-Path $Output 'stdout.log') -RedirectStandardError (Join-Path $Output 'stderr.log')
    $window=Get-NormaWindow $process
    Focus-NormaWindow $window
    Start-Sleep -Seconds 4
    Save-NormaScreenshot $window (Join-Path $Output '01-start.png')
    Send-NormaChord @(0x11,0x4E) # Ctrl+N must be the Windows workspace command.
    Start-Sleep -Seconds 1
    Save-NormaScreenshot $window (Join-Path $Output '02-create-dialog.png')
    $name="Runner 中文 workspace $env:GITHUB_RUN_ID"
    [NormaDesktop]::Text($name)
    Send-NormaChord @(0x0D)
    Start-Sleep -Seconds 3
    Save-NormaScreenshot $window (Join-Path $Output '03-created.png')
    # Read-only persistence evidence supplements the actual native input/screenshots.
    $db=Join-Path $env:USERPROFILE 'Library/Application Support/Norma OS/app.sqlite'
    python scripts/windows/read-workspace.py --database $db --name $name --output (Join-Path $Output 'created.json')
    if ($LASTEXITCODE -ne 0) { throw 'Native Ctrl+N and Unicode input did not create the requested workspace' }
    Close-NormaWindow $process $window
    $process.Dispose(); $process=$null
    $process=Start-Process (Join-Path $root 'norma-os.exe') -WorkingDirectory $root -PassThru -RedirectStandardOutput (Join-Path $Output 'reopen-stdout.log') -RedirectStandardError (Join-Path $Output 'reopen-stderr.log')
    $window=Get-NormaWindow $process
    Focus-NormaWindow $window
    Start-Sleep -Seconds 4
    Save-NormaScreenshot $window (Join-Path $Output '04-reopened.png')
    python scripts/windows/read-workspace.py --database $db --name $name --output (Join-Path $Output 'reopened.json')
    if ($LASTEXITCODE -ne 0) { throw 'Created workspace was not persisted after reopen' }
    Close-NormaWindow $process $window
    $report.status='PASS'
} catch { $report.error=$_.Exception.Message; throw }
finally {
    $report | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Output 'report.json') -Encoding utf8
    if ($process) { $process.Refresh(); if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit(10000) | Out-Null }; $process.Dispose() }
    $logs=Join-Path $env:USERPROFILE '.norma/logs'
    if (Test-Path $logs) { Copy-Item $logs (Join-Path $Output 'app-logs') -Recurse }
}
