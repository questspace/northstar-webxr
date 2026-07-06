# Starts the Ultraleap -> WebSocket bridge that the browser app expects
# at ws://localhost:6437/v6.json (LeapJS v6 protocol).
#
# Why this is needed: Ultraleap Gemini/Hyperion dropped the built-in browser
# WebSocket. The prebuilt Ultraleap-Tracking-WS.exe re-exposes v6 tracking
# frames from the local tracking service so LeapJS-style clients keep working.

$ErrorActionPreference = 'Stop'

$bridgeDir = Join-Path $PSScriptRoot 'ultraleap-websocket\build-win-vcpkg\Release'
$bridgeExe = Join-Path $bridgeDir 'Ultraleap-Tracking-WS.exe'
$logDir = Join-Path $PSScriptRoot 'tmp'

if (-not (Test-Path $bridgeExe)) {
    throw "Bridge not found at $bridgeExe. Build ultraleap-websocket first."
}

# The tracking service must be running (and healthy) or no frames are produced.
$svc = Get-Service -Name 'UltraleapTracking' -ErrorAction SilentlyContinue
if ($null -eq $svc) {
    Write-Warning "UltraleapTracking service is not installed."
} elseif ($svc.Status -ne 'Running') {
    Write-Host "Starting UltraleapTracking service..."
    try {
        Start-Service -Name 'UltraleapTracking'
    } catch {
        Write-Warning "Could not start UltraleapTracking (needs admin). Start it manually, then rerun."
    }
}

# Reuse the running bridge if one is already up.
if (Get-Process -Name 'Ultraleap-Tracking-WS' -ErrorAction SilentlyContinue) {
    Write-Host "Bridge already running on ws://localhost:6437/v6.json"
    return
}

New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$proc = Start-Process -FilePath $bridgeExe -WorkingDirectory $bridgeDir `
    -RedirectStandardOutput (Join-Path $logDir 'ultraleap-ws.log') `
    -RedirectStandardError (Join-Path $logDir 'ultraleap-ws.err.log') `
    -WindowStyle Hidden -PassThru

Start-Sleep -Seconds 2
if ($proc.HasExited) {
    throw "Bridge exited immediately. Check $logDir\ultraleap-ws.err.log"
}

Write-Host "Ultraleap bridge started (PID $($proc.Id)) on ws://localhost:6437/v6.json"
