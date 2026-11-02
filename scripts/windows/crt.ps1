function Find-NormaCrtDirectory {
    param([Parameter(Mandatory)][string]$VisualStudio)
    $runtime = Get-ChildItem (Join-Path $VisualStudio 'VC/Redist/MSVC') -Directory |
        ForEach-Object { Join-Path $_.FullName 'x64/Microsoft.VC143.CRT' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Container } |
        Sort-Object | Select-Object -Last 1
    if (-not $runtime) { throw 'Visual C++ x64 runtime redistributables are missing' }
    return $runtime
}
