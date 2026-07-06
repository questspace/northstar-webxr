<#
.SYNOPSIS
    Starts all Vibestar services and streams their logs into one console with
    per-service colored prefixes. Press Ctrl+C to stop everything.

.DESCRIPTION
    Services:
      xr50  XVisio XR50 6DOF bridge      ws://localhost:8080
      leap  Ultraleap v6 hand bridge     ws://localhost:6437/v6.json
      web   Vite dev server (browser UI) http://localhost:5173

    Each service's output is captured to logs\<name>.log and mirrored here.

.EXAMPLE
    ./start-all.ps1
    ./start-all.ps1 -NoXR50 -NoLeap   # web only (e.g. simulator mode)
#>
[CmdletBinding()]
param(
    [switch]$NoXR50,
    [switch]$NoLeap,
    [switch]$NoWeb
)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$logDir = Join-Path $root 'logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

$colors = @{ sys = 'DarkGray'; xr50 = 'Cyan'; leap = 'Green'; web = 'Yellow' }
function Write-Tagged([string]$Name, [string]$Message) {
    $color = $colors[$Name]; if (-not $color) { $color = 'White' }
    Write-Host ("[{0}] {1}" -f $Name, $Message) -ForegroundColor $color
}

# The Ultraleap v6 bridge needs the tracking service running to get frames.
if (-not $NoLeap) {
    $svc = Get-Service -Name 'UltraleapTracking' -ErrorAction SilentlyContinue
    if ($svc -and $svc.Status -ne 'Running') {
        Write-Tagged 'sys' 'Starting UltraleapTracking service...'
        try { Start-Service -Name 'UltraleapTracking' }
        catch { Write-Tagged 'sys' 'WARN: could not start UltraleapTracking (needs admin). Start it manually.' }
    }
}

# Assemble the service definitions, warning about anything not built yet.
$defs = @()
if (-not $NoXR50) {
    $exe = Join-Path $root 'xvisio-rs\target\release\examples\server.exe'
    if (Test-Path $exe) { $defs += @{ Name = 'xr50'; File = $exe; Args = @(); Cwd = (Join-Path $root 'xvisio-rs') } }
    else { Write-Tagged 'sys' "WARN: XR50 bridge missing. Build: cargo build --release --example server" }
}
if (-not $NoLeap) {
    $exe = Join-Path $root 'ultraleap-websocket\build-win-vcpkg\Release\Ultraleap-Tracking-WS.exe'
    if (Test-Path $exe) { $defs += @{ Name = 'leap'; File = $exe; Args = @(); Cwd = (Split-Path $exe) } }
    else { Write-Tagged 'sys' "WARN: Ultraleap bridge missing at $exe" }
}
if (-not $NoWeb) {
    $npm = (Get-Command npm.cmd -ErrorAction SilentlyContinue).Source
    if ($npm) { $defs += @{ Name = 'web'; File = $npm; Args = @('run', 'dev'); Cwd = (Join-Path $root 'visual-test') } }
    else { Write-Tagged 'sys' 'WARN: npm not found in PATH.' }
}

if ($defs.Count -eq 0) { Write-Tagged 'sys' 'Nothing to start.'; return }

# Start each service, capturing stdout/stderr to per-service log files.
$procs = @()
$files = [ordered]@{}   # log file -> service name
$offsets = @{}
foreach ($d in $defs) {
    $log = Join-Path $logDir ("{0}.log" -f $d.Name)
    Set-Content -Path $log -Value '' -NoNewline
    $argStr = ($d.Args -join ' ')
    Write-Tagged 'sys' ("starting {0}: {1} {2}" -f $d.Name, $d.File, $argStr)
    # Merge stdout+stderr into one log via cmd redirection so all output for a
    # service lands in a single file (repo paths contain no spaces).
    $cmdLine = "$($d.File) $argStr > $log 2>&1"
    $p = Start-Process -FilePath $env:ComSpec -ArgumentList '/c', $cmdLine `
        -WorkingDirectory $d.Cwd -WindowStyle Hidden -PassThru
    $procs += @{ Name = $d.Name; Proc = $p }
    $files[$log] = $d.Name; $offsets[$log] = 0
}

Write-Tagged 'sys' ("Started {0} service(s). Logs: {1}" -f $procs.Count, $logDir)
Write-Tagged 'sys' 'URLs: web http://localhost:5173 | xr50 ws://localhost:8080 | leap ws://localhost:6437/v6.json'
Write-Tagged 'sys' 'Press Ctrl+C to stop all services.'

# Tail every log file, printing new lines with the owning service's prefix.
function Read-NewLines {
    foreach ($f in @($files.Keys)) {
        if (-not (Test-Path $f)) { continue }
        try {
            $fs = [System.IO.File]::Open($f, 'Open', 'Read', 'ReadWrite')
            [void]$fs.Seek($offsets[$f], 'Begin')
            $reader = New-Object System.IO.StreamReader($fs)
            $chunk = $reader.ReadToEnd()
            $offsets[$f] = $fs.Position
            $reader.Dispose(); $fs.Dispose()
        }
        catch { continue }
        if ($chunk) {
            foreach ($line in ($chunk -split "`r?`n")) {
                if ($line.Trim().Length -gt 0) { Write-Tagged $files[$f] $line }
            }
        }
    }
}

try {
    while ($true) {
        Read-NewLines
        if (-not ($procs | Where-Object { -not $_.Proc.HasExited })) {
            Read-NewLines
            Write-Tagged 'sys' 'All services exited.'
            break
        }
        Start-Sleep -Milliseconds 250
    }
}
finally {
    Write-Tagged 'sys' 'Stopping services...'
    foreach ($p in $procs) {
        if (-not $p.Proc.HasExited) {
            # /T stops the whole process tree (e.g. npm -> node -> vite).
            & taskkill.exe /PID $p.Proc.Id /T /F *> $null
        }
    }
    Read-NewLines
    Write-Tagged 'sys' 'Done.'
}
