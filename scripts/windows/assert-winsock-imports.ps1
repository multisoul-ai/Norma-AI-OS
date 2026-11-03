param([Parameter(Mandatory)][string]$Package, [Parameter(Mandatory)][string]$Output)
$ErrorActionPreference='Stop'
New-Item -ItemType Directory -Force $Output | Out-Null
$Output=(Resolve-Path $Output).Path
$root=(Resolve-Path $Package).Path
$vswhere="${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$vs=& $vswhere -latest -products '*' -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Visual Studio metadata unavailable for PE import inspection' }
$dumpbin=Get-ChildItem "$vs/VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe" -File |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $dumpbin) { throw 'Native x64 dumpbin is unavailable; import checks were not executed' }
$manifest=Get-Content (Join-Path $root 'manifest.json') -Raw | ConvertFrom-Json
$report=@{ status='FAIL'; source_sha=$manifest.source_sha; probe_sha=$env:GITHUB_SHA; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT; zip_sha256=$env:CANDIDATE_ZIP_SHA256; tool=$dumpbin.FullName; checks=@(); scope='Known mandatory import regression only; not complete Windows 10 device acceptance' }
try {
    $rejected=$false
    foreach ($name in @('norma-os.exe','norma.exe','multisoul-agent-hook.exe','norma-browser-auth-host.exe')) {
        $path=Join-Path $root $name
        $text=(& $dumpbin.FullName /imports $path 2>&1) -join "`n"
        if ($LASTEXITCODE -ne 0) { throw "PE import inspection failed: $name" }
        $text | Set-Content (Join-Path $Output "$name.imports.log") -Encoding utf8
        $blocked=[bool]($text -match '\bProcessSocketNotifications\b')
        $report.checks+=@{ binary=$name; mandatory_ProcessSocketNotifications=$blocked }
        if ($blocked) { $rejected=$true }
    }
    if ($rejected) { throw 'Mandatory ProcessSocketNotifications import prevents loading on Windows 10 build 19045' }
    $report.status='PASS'
} catch { $report.error=$_.Exception.Message; throw }
finally { $report | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Output 'report.json') -Encoding utf8 }
