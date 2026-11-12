param(
    [string]$Version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'VERSION') -Raw).Trim(),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist')
)

$ErrorActionPreference = 'Stop'
$versionNumber = [version]$Version
if ($versionNumber.Build -lt 0 -or $versionNumber.Revision -ge 0 -or
    $versionNumber.Major -gt 255 -or $versionNumber.Minor -gt 255 -or $versionNumber.Build -gt 65534) {
    throw 'Use a three-part MSI version such as 0.4.0.'
}
$Version = $versionNumber.ToString(3)

$luaDir = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Lua\5.1'
if ($env:LUA51_DIR) { $luaDir = $env:LUA51_DIR }
$runtime = Join-Path $luaDir 'lua5.1.dll'
$license = Join-Path $luaDir 'LICENSE.txt'
$importLibrary = Join-Path $luaDir 'lib\lua5.1.lib'

foreach ($file in $runtime, $license, $importLibrary) {
    if (!(Test-Path -LiteralPath $file -PathType Leaf)) {
        throw 'Set LUA51_DIR to the 32-bit Lua for Windows 5.1 folder containing the DLL, import library, and license.'
    }
}

$cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (!$cargo -and $env:CARGO_HOME) {
    $cargo = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
}
if (!$cargo -or !(Test-Path -LiteralPath $cargo -PathType Leaf)) {
    throw 'Install Rust with rustup and add the i686-pc-windows-msvc target.'
}

$previousLib = $env:LUA_LIB
$previousName = $env:LUA_LIB_NAME
$previousVersion = $env:LUAHOTKEYS_BUILD_VERSION
$targetDir = Join-Path $PSScriptRoot 'target'
try {
    $env:LUA_LIB = Split-Path -Parent $importLibrary
    $env:LUA_LIB_NAME = 'lua5.1'
    $env:LUAHOTKEYS_BUILD_VERSION = $Version
    & $cargo build --locked --release --target i686-pc-windows-msvc --target-dir $targetDir --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw "Rust build failed with exit code $LASTEXITCODE." }
}
finally {
    $env:LUA_LIB = $previousLib
    $env:LUA_LIB_NAME = $previousName
    $env:LUAHOTKEYS_BUILD_VERSION = $previousVersion
}

$artifacts = Join-Path $targetDir 'i686-pc-windows-msvc\release'
New-Item -Path $OutputDirectory -ItemType Directory -Force | Out-Null
$executable = Join-Path $OutputDirectory 'LuaHotkeys.exe'
Copy-Item -LiteralPath (Join-Path $artifacts 'luahotkeys.exe') -Destination $executable -Force
Copy-Item -LiteralPath (Join-Path $artifacts 'luahotkeys-updater.exe') -Destination (Join-Path $OutputDirectory 'LuaHotkeysUpdater.exe') -Force
Copy-Item -LiteralPath $runtime -Destination $OutputDirectory -Force
# Keep the notice filename expected by the existing portable updater.
$metadata = & $cargo metadata --locked --offline --format-version 1 --filter-platform i686-pc-windows-msvc --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Could not read the Rust dependency metadata.' }
$packages = ($metadata | ConvertFrom-Json).packages | Where-Object source | Sort-Object name, version
$texts = New-Object System.Collections.Specialized.OrderedDictionary ([StringComparer]::Ordinal)
$mlua = $packages | Where-Object name -eq 'mlua' | Select-Object -First 1
foreach ($package in $packages) {
    $directory = Split-Path -Parent $package.manifest_path
    $files = @(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -Match '^(LICENSE|COPYING|NOTICE|COPYRIGHT|UNLICENSE)([._-]|$)' | Sort-Object Name)
    if ($package.license_file) {
        $files += Get-Item -LiteralPath (Join-Path $directory $package.license_file)
    }
    if (!$files -and $package.name -eq 'mlua-sys' -and $mlua) {
        # mlua-sys shares the MIT license of its mlua workspace.
        $files = @(Get-Item -LiteralPath (Join-Path (Split-Path -Parent $mlua.manifest_path) 'LICENSE'))
    }
    if ($package.name -eq 'imgui-sys') {
        $files += Get-Item -LiteralPath (Join-Path $directory 'third-party\imgui-master\imgui\LICENSE.txt')
    }
    $label = "$($package.name) $($package.version) ($($package.license))"
    foreach ($file in ($files | Sort-Object FullName -Unique)) {
        $licenseText = [IO.File]::ReadAllText($file.FullName).TrimEnd()
        if (!$texts.Contains($licenseText)) {
            $texts[$licenseText] = New-Object 'System.Collections.Generic.List[string]'
        }
        if (!$texts[$licenseText].Contains($label)) {
            $texts[$licenseText].Add($label)
        }
    }
}
$notices = New-Object System.Text.StringBuilder
[void]$notices.AppendLine([IO.File]::ReadAllText($license).TrimEnd())
[void]$notices.AppendLine().AppendLine('Rust dependency license notices').AppendLine()
foreach ($package in ($packages | Where-Object { $_.name -in 'full_moon', 'full_moon_derive' })) {
    [void]$notices.AppendLine("$($package.name) $($package.version) ($($package.license))")
    [void]$notices.AppendLine('License: https://www.mozilla.org/MPL/2.0/')
    [void]$notices.AppendLine("Source: https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download").AppendLine()
}
foreach ($licenseText in $texts.Keys) {
    [void]$notices.AppendLine(($texts[$licenseText] -join ', ')).AppendLine().AppendLine($licenseText).AppendLine()
}
$rustc = Join-Path (Split-Path -Parent $cargo) 'rustc.exe'
if (Test-Path -LiteralPath $rustc) {
    $sysroot = & $rustc --print sysroot
    if ($LASTEXITCODE -eq 0) {
        $report = Join-Path $sysroot 'share\doc\rust\COPYRIGHT-library.html'
        if (Test-Path -LiteralPath $report) {
            [void]$notices.AppendLine('Rust standard library copyright report (HTML)').AppendLine()
            [void]$notices.AppendLine([IO.File]::ReadAllText($report))
        }
    }
}
$noticePath = Join-Path $OutputDirectory 'Lua-LICENSE.txt'
[IO.File]::WriteAllText($noticePath, $notices.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Output $executable
