$ErrorActionPreference = 'Stop'
$appArguments = @($args)
$project = $PSScriptRoot

$cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (!$cargo) {
    $rustupCargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
    if (Test-Path -LiteralPath $rustupCargo -PathType Leaf) {
        $cargo = $rustupCargo
    }
}
if (!$cargo) {
    $toolchainConfig = Join-Path $project 'target/dev-toolchain.json'
    if (Test-Path -LiteralPath $toolchainConfig -PathType Leaf) {
        $toolchain = Get-Content -LiteralPath $toolchainConfig -Raw | ConvertFrom-Json
        $env:CARGO_HOME = $toolchain.cargoHome
        $env:RUSTUP_HOME = $toolchain.rustupHome
        $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    }
}
if (!$cargo -or !(Test-Path -LiteralPath $cargo -PathType Leaf)) {
    throw 'Install Rust with rustup and add the i686-pc-windows-msvc target to run LuaHotkeys from source.'
}

$lua = $env:LUA51_DIR
if (!$lua) {
    $lua = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Lua/5.1'
}
$importLibrary = Join-Path $lua 'lib/lua5.1.lib'
$runtime = Join-Path $lua 'lua5.1.dll'
if (!(Test-Path -LiteralPath $importLibrary) -or !(Test-Path -LiteralPath $runtime)) {
    throw 'Install the 32-bit Lua 5.1 runtime and import library, or point LUA51_DIR to them.'
}

$env:LUA_LIB = Split-Path -Parent $importLibrary
$env:LUA_LIB_NAME = 'lua5.1'
$env:PATH = "$lua;$env:PATH"
Push-Location -LiteralPath $project
try {
    & $cargo run --locked --target i686-pc-windows-msvc --bin luahotkeys -- @appArguments
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
