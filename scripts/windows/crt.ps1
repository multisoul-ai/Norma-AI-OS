function Find-NormaCrtDirectory {
    param([Parameter(Mandatory)][string]$VisualStudio)
    $root = Join-Path $VisualStudio 'VC/Redist/MSVC'
    $versionFile = Join-Path $VisualStudio 'VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt'
    if (Test-Path -LiteralPath $versionFile -PathType Leaf) {
        $versionText = (Get-Content -LiteralPath $versionFile -Raw).Trim()
        $version = $null
        if (-not [Version]::TryParse($versionText, [ref]$version)) { throw 'Visual Studio declared an invalid CRT version' }
        $versions = @(Get-Item -LiteralPath (Join-Path $root $versionText) -ErrorAction Stop)
    } else {
        $versions = @(Get-ChildItem -LiteralPath $root -Directory)
    }
    $runtime = $versions |
        ForEach-Object { Get-ChildItem (Join-Path $_.FullName 'x64') -Directory -ErrorAction SilentlyContinue } |
        Where-Object { $_.Name -match '^Microsoft\.VC[0-9]+\.CRT$' } |
        Sort-Object -Property @{ Expression = { [Version]$_.Parent.Parent.Name } } -Descending |
        Select-Object -First 1
    if (-not $runtime) { throw 'Visual C++ x64 runtime redistributables are missing' }
    foreach ($name in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll')) {
        if (-not (Test-Path -LiteralPath (Join-Path $runtime.FullName $name) -PathType Leaf)) {
            throw "The selected CRT is incomplete: $name; refusing to ship an older runtime"
        }
    }
    return $runtime.FullName
}
