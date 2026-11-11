$ErrorActionPreference = 'Stop'
$luaDir = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Lua\5.1'
if ($env:LUA51_DIR) { $luaDir = $env:LUA51_DIR }
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe'
$outputDir = Join-Path $PSScriptRoot 'dist'
$runtime = Join-Path $luaDir 'lua5.1.dll'
$license = Join-Path $luaDir 'LICENSE.txt'

if (!(Test-Path -LiteralPath $runtime -PathType Leaf) -or !(Test-Path -LiteralPath $license -PathType Leaf)) {
    throw 'Set LUA51_DIR to the folder containing the 32-bit lua5.1.dll and its LICENSE.txt.'
}

New-Item -Path $outputDir -ItemType Directory -Force | Out-Null
$executable = Join-Path $outputDir 'LuaHotkeys.exe'
$demo = Join-Path $PSScriptRoot 'example.lua'
$sources = @('Program.cs', 'LuaWorker.cs', 'WindowsInput.cs', 'LuaEditor.cs', 'FileAssociation.cs') | ForEach-Object { Join-Path $PSScriptRoot $_ }
& $compiler /nologo /target:winexe /platform:x86 /optimize+ /codepage:65001 /langversion:5 /r:System.Windows.Forms.dll /r:System.Drawing.dll "/out:$executable" "/resource:$demo,example.lua" $sources
if ($LASTEXITCODE -ne 0) { throw "Build failed with exit code $LASTEXITCODE." }

Copy-Item -LiteralPath $runtime -Destination $outputDir -Force
Copy-Item -LiteralPath $license -Destination (Join-Path $outputDir 'Lua-LICENSE.txt') -Force
Write-Output $executable
