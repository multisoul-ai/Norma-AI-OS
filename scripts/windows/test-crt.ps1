$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/crt.ps1"
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('Norma CRT 验收 ' + [Guid]::NewGuid().ToString('N'))
function Add-RuntimeFixture([string]$Version, [string]$Tag, [string]$Architecture = 'x64') {
    $path = Join-Path $fixture "VC/Redist/MSVC/$Version/$Architecture/$Tag"
    New-Item -ItemType Directory -Force $path | Out-Null
    foreach ($name in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll')) {
        Set-Content -LiteralPath (Join-Path $path $name) -Value 'discovery fixture only'
    }
    return $path
}
try {
    $older = Add-RuntimeFixture '14.44.35112' 'Microsoft.VC143.CRT'
    $newer = Add-RuntimeFixture '14.51.36231' 'Microsoft.VC145.CRT'
    $arm = Add-RuntimeFixture '14.60.10000' 'Microsoft.VC146.CRT' 'arm64'
    $selected = Find-NormaCrtDirectory -VisualStudio $fixture
    if ($selected -ne $newer) { throw "Expected latest complete x64 CRT $newer; received $selected" }
    if ($selected -eq $arm) { throw 'An ARM64 runtime cannot be packaged in x64' }
    Remove-Item -LiteralPath (Join-Path $newer 'vcruntime140_1.dll')
    $incompleteRejected = $false
    try { Find-NormaCrtDirectory -VisualStudio $fixture | Out-Null } catch { $incompleteRejected = $true }
    if (-not $incompleteRejected) { throw 'A partially installed latest x64 runtime must fail closed instead of shipping the older runtime' }
    Remove-Item -LiteralPath (Join-Path $older 'msvcp140.dll')
    $rejected = $false
    try { Find-NormaCrtDirectory -VisualStudio $fixture | Out-Null } catch { $rejected = $true }
    if (-not $rejected) { throw 'No complete x64 runtime must fail closed' }
    Write-Output 'PASS: latest native CRT, architecture, partial-directory and missing-runtime contracts'
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
