# Builds xvsdk-bridge.exe next to the SDK's DLLs (tools/xvsdk/sdk/bin), which it needs at run time.
#   powershell -ExecutionPolicy Bypass -File xvsdk-bridge\build.ps1
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$sdk = Join-Path $here '..\tools\xvsdk\sdk' | Resolve-Path
if (-not (Test-Path "$sdk\bin\xvsdk.dll")) { throw 'Run tools\xvsdk\fetch-xvsdk.ps1 first' }
$build = Join-Path $here 'build'
$implib = Join-Path $build 'implib'
New-Item -ItemType Directory -Force $implib | Out-Null

# MSVC environment (dumpbin, lib) from the newest Visual Studio / Build Tools.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'

# xvsdk.lib from the DLL's exports (the SDK ships the DLL only).
$def = Join-Path $implib 'xvsdk.def'
cmd /c "`"$vcvars`" >nul && dumpbin /nologo /exports `"$sdk\bin\xvsdk.dll`"" |
    ForEach-Object { if ($_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(\S+)') { $Matches[1] } } |
    Set-Variable names
@('LIBRARY xvsdk', 'EXPORTS') + $names | Set-Content -Encoding ascii $def
cmd /c "`"$vcvars`" >nul && lib /nologo /machine:x64 /def:`"$def`" /out:`"$implib\xvsdk.lib`"" | Out-Null
"xvsdk.lib: $($names.Count) exports"

cmake -S $here -B $build -G 'Visual Studio 17 2022' -A x64 | Out-Null
cmake --build $build --config Release
Copy-Item "$build\Release\xvsdk-bridge.exe" "$sdk\bin\" -Force
"built $sdk\bin\xvsdk-bridge.exe"
