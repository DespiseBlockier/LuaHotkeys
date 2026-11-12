param(
    [string]$Version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'VERSION') -Raw).Trim(),
    [string]$WixBin = $env:WIX_BIN,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist')
)

$ErrorActionPreference = 'Stop'
if (!$WixBin) {
    $candleCommand = Get-Command candle.exe -ErrorAction SilentlyContinue
    if ($candleCommand) { $WixBin = Split-Path -Parent $candleCommand.Source }
}
if (!$WixBin) {
    throw 'Set WIX_BIN to the WiX 3.14 bin folder, or put candle.exe and light.exe on PATH.'
}
$candle = Join-Path $WixBin 'candle.exe'
$light = Join-Path $WixBin 'light.exe'
$uiLibrary = Join-Path $WixBin 'WixUIExtension.dll'
$manifest = Join-Path $PSScriptRoot 'Installer.wxs'
$stage = Join-Path ([IO.Path]::GetTempPath()) ('luahotkeys-build-' + [Guid]::NewGuid().ToString('N'))
$wixObject = Join-Path $stage 'Installer.wixobj'

try {
    & (Join-Path $PSScriptRoot 'build.ps1') -Version $Version -OutputDirectory $stage | Out-Null
    $Version = ([version]$Version).ToString(3)
    New-Item -Path $OutputDirectory -ItemType Directory -Force | Out-Null
    $msi = Join-Path $OutputDirectory "LuaHotkeys-$Version-x86.msi"

    & $candle -nologo -arch x86 "-dProductVersion=$Version" "-dAppDirectory=$stage" "-dProjectDirectory=$PSScriptRoot" -out $wixObject $manifest
    if ($LASTEXITCODE -ne 0) { throw "WiX compile failed with exit code $LASTEXITCODE." }
    & $light -nologo -ext $uiLibrary -cultures:en-us -spdb -out $msi $wixObject
    if ($LASTEXITCODE -ne 0) { throw "WiX link failed with exit code $LASTEXITCODE." }
    Write-Output $msi
}
finally {
    if (Test-Path -LiteralPath $stage) {
        foreach ($file in 'LuaHotkeys.exe', 'LuaHotkeysUpdater.exe', 'lua5.1.dll', 'Lua-LICENSE.txt', 'Installer.wixobj') {
            Remove-Item -LiteralPath (Join-Path $stage $file) -ErrorAction SilentlyContinue
        }
        if (!(Get-ChildItem -LiteralPath $stage -Force)) {
            Remove-Item -LiteralPath $stage
        }
    }
}
