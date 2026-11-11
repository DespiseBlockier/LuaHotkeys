param(
    [string]$Version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'VERSION') -Raw).Trim(),
    [string]$WixBin = $env:WIX_BIN,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist')
)

$ErrorActionPreference = 'Stop'
$stage = Join-Path ([IO.Path]::GetTempPath()) ('luahotkeys-release-' + [Guid]::NewGuid().ToString('N'))
$programFiles = @('LuaHotkeys.exe', 'LuaHotkeysUpdater.exe', 'lua5.1.dll', 'Lua-LICENSE.txt')
try {
    & (Join-Path $PSScriptRoot 'build.ps1') -Version $Version -OutputDirectory $stage | Out-Null
    $Version = ([version]$Version).ToString(3)
    & (Join-Path $PSScriptRoot 'build-installer.ps1') -Version $Version -WixBin $WixBin -OutputDirectory $OutputDirectory

    $files = $programFiles | ForEach-Object { Join-Path $stage $_ }
    $files += Join-Path $PSScriptRoot 'example.lua'
    $files += Join-Path $PSScriptRoot 'README.md'
    $portable = Join-Path $OutputDirectory "LuaHotkeys-$Version-win-x86.zip"
    Compress-Archive -LiteralPath $files -DestinationPath $portable -Force
    Write-Output $portable
}
finally {
    if (Test-Path -LiteralPath $stage) {
        foreach ($file in $programFiles) {
            Remove-Item -LiteralPath (Join-Path $stage $file) -ErrorAction SilentlyContinue
        }
        if (!(Get-ChildItem -LiteralPath $stage -Force)) {
            Remove-Item -LiteralPath $stage
        }
    }
}
