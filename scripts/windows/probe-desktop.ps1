param([Parameter(Mandatory)][string]$Package, [Parameter(Mandatory)][string]$Output, [Parameter(Mandatory)][string]$ExpectedSource)
$ErrorActionPreference='Stop'
New-Item -ItemType Directory -Force $Output | Out-Null
$Output=(Resolve-Path $Output).Path
$root=(Resolve-Path $Package).Path
$report=@{ status='FAIL'; source_sha=$null; probe_sha=$env:GITHUB_SHA; producer_run=$env:CANDIDATE_RUN; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; zip_sha256=$env:CANDIDATE_ZIP_SHA256; scope='Same packaged App: helpers, native startup, exact Unicode workspace query, normal close and reopen with identical ID; no model task' }
try {
    $manifest=Get-Content (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json
    $report.source_sha=$manifest.source_sha
    if ($manifest.source_sha -ne $ExpectedSource) { throw 'Candidate source identity mismatch' }
    Copy-Item (Join-Path $root 'manifest.json') (Join-Path $Output 'candidate-manifest.json')
    & "$PSScriptRoot/smoke-package.ps1" -Package $root -Output (Join-Path $Output 'package-smoke') -ExpectedSourceSha $ExpectedSource
    # Reuse the proven native path, including ready/error checks and exact ID comparison.
    $nativeOutput=Join-Path $Output 'workspace'
    & "$PSScriptRoot/probe-startup.ps1" -Executable (Join-Path $root 'norma-os.exe') -Output $nativeOutput -ExitMode workspace-reopen -SourceSha $manifest.source_sha
    $native=Get-Content (Join-Path $nativeOutput 'report.json') -Raw | ConvertFrom-Json
    if ($native.status -ne 'PASS' -or -not $native.workspace_persisted) { throw 'Native workspace persistence acceptance failed' }
    $report.workspace_name=$native.workspace_name
    $report.workspace_id=$native.workspace_id
    $report.workspace_persisted=$true
    $report.visual_verification='PENDING'
    $report.status='PASS'
} catch { $report.error=$_.Exception.Message; throw }
finally {
    $report | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Output 'report.json') -Encoding utf8
}
