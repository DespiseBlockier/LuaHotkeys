param(
    [string]$Version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'VERSION') -Raw).Trim(),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist')
)

$ErrorActionPreference = 'Stop'
$versionNumber = [version]$Version
if ($versionNumber.Build -lt 0 -or $versionNumber.Revision -ge 0 -or
    $versionNumber.Major -gt 255 -or $versionNumber.Minor -gt 255 -or $versionNumber.Build -gt 65534) {
    throw 'Use a three-part MSI version such as 0.2.0.'
}
$Version = $versionNumber.ToString(3)

$luaDir = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Lua\5.1'
if ($env:LUA51_DIR) { $luaDir = $env:LUA51_DIR }
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe'
$runtime = Join-Path $luaDir 'lua5.1.dll'
$license = Join-Path $luaDir 'LICENSE.txt'

if (!(Test-Path -LiteralPath $runtime -PathType Leaf) -or !(Test-Path -LiteralPath $license -PathType Leaf)) {
    throw 'Set LUA51_DIR to the folder containing the 32-bit lua5.1.dll and its LICENSE.txt.'
}

New-Item -Path $OutputDirectory -ItemType Directory -Force | Out-Null
$executable = Join-Path $OutputDirectory 'LuaHotkeys.exe'
$demo = Join-Path $PSScriptRoot 'example.lua'
$sources = @('Program.cs', 'LuaWorker.cs', 'WindowsInput.cs', 'LuaEditor.cs', 'FileAssociation.cs') | ForEach-Object { Join-Path $PSScriptRoot $_ }
$assemblyInfo = [IO.Path]::GetTempFileName()
try {
    $metadata = @"
using System.Reflection;
[assembly: AssemblyTitle("LuaHotkeys")]
[assembly: AssemblyProduct("LuaHotkeys")]
[assembly: AssemblyVersion("$Version.0")]
[assembly: AssemblyFileVersion("$Version.0")]
[assembly: AssemblyInformationalVersion("$Version")]
"@
    [IO.File]::WriteAllText($assemblyInfo, $metadata)
    & $compiler /nologo /target:winexe /platform:x86 /optimize+ /codepage:65001 /langversion:5 /r:System.Windows.Forms.dll /r:System.Drawing.dll "/out:$executable" "/resource:$demo,example.lua" $sources $assemblyInfo
    if ($LASTEXITCODE -ne 0) { throw "Build failed with exit code $LASTEXITCODE." }
}
finally {
    Remove-Item -LiteralPath $assemblyInfo -ErrorAction SilentlyContinue
}

Copy-Item -LiteralPath $runtime -Destination $OutputDirectory -Force
Copy-Item -LiteralPath $license -Destination (Join-Path $OutputDirectory 'Lua-LICENSE.txt') -Force
Write-Output $executable
