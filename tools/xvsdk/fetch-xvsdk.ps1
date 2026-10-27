# Downloads the official XVisio XVSDK 3.2.0 for Windows (MSVC 2019, x64) from the vendor's
# server, checks its hash and unpacks it into tools/xvsdk/sdk without running the installer.
# The NSIS installer is unpacked with a portable 7-Zip (official 7zr.exe + 7-Zip setup archive).
#   powershell -ExecutionPolicy Bypass -File tools\xvsdk\fetch-xvsdk.ps1
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$cache = Join-Path $here 'cache'
$sdk = Join-Path $here 'sdk'
$url = 'https://download.xvisiotech.com/sdk-download/last_version/Select_Platform_Download/For%20windows/XVSDK-3.2.0-msvc2019-x64.exe'
$sha256 = '3D2509BA0A5D1495060A9FC0B39C7FB4E5D3E46398123A726C1C131F2C409009'
New-Item -ItemType Directory -Force $cache | Out-Null

$installer = Join-Path $cache 'XVSDK-3.2.0-msvc2019-x64.exe'
if (-not (Test-Path $installer) -or (Get-FileHash $installer).Hash -ne $sha256) {
    # The vendor's TLS certificate has expired; the SHA-256 check below is what vouches for the file.
    & curl.exe -skL -o $installer $url
    if ((Get-FileHash $installer).Hash -ne $sha256) { throw "XVSDK hash mismatch: the vendor changed the file, check it before updating `$sha256" }
}

$sevenZip = Join-Path $cache '7zip\7z.exe'
if (-not (Test-Path $sevenZip)) {
    Invoke-WebRequest -UseBasicParsing 'https://www.7-zip.org/a/7zr.exe' -OutFile (Join-Path $cache '7zr.exe')
    Invoke-WebRequest -UseBasicParsing 'https://www.7-zip.org/a/7z2409-x64.exe' -OutFile (Join-Path $cache '7z-setup.exe')
    & (Join-Path $cache '7zr.exe') x (Join-Path $cache '7z-setup.exe') "-o$(Join-Path $cache '7zip')" -y | Out-Null
}

if (Test-Path $sdk) { Remove-Item -Recurse -Force $sdk }
& $sevenZip x $installer "-o$sdk" -y '-x!$PLUGINSDIR' '-x!Uninstall.exe' | Out-Null
if (-not (Test-Path (Join-Path $sdk 'bin\xvsdk.dll'))) { throw 'unpacking failed: bin\xvsdk.dll missing' }
"XVSDK unpacked to $sdk"
"Samples: $sdk\bin\all_stream.exe, $sdk\bin\demo-api.exe (run from the bin folder)"
