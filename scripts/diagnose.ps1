param(
    [switch]$Bundle,
    [string]$BundlePath = ""
)

. (Join-Path $PSScriptRoot "common.ps1")

$Logs = @("launcher", "install", "update", "backend", "backend-console", "websocket", "crash")
$script:Report = New-Object System.Collections.Generic.List[string]
$script:Blocked = $false

function Say([string]$Text) { $script:Report.Add($Text); Write-Host "  $Text" }
function Pass([string]$Text) { Say "[OK] $Text" }
function Flag([string]$Text) { $script:Blocked = $true; Say "[X] $Text" }
function Heading([string]$Title) {
    $script:Report.Add("")
    $script:Report.Add("== $Title ==")
    Write-Host ""
    Write-Host "==> $Title" -ForegroundColor Cyan
}

function Read-Json([string]$Path) { return Get-Content $Path -Raw -Encoding UTF8 | ConvertFrom-Json }

function Find-ProcessById([object]$Id) {
    if (-not "$Id") { return $null }
    return Get-CimInstance Win32_Process -Filter "ProcessId=$Id" -ErrorAction SilentlyContinue
}

function Get-PortOwner([int]$Port) {
    $listener = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $listener) { return "free" }
    $owner = $listener.OwningProcess
    # The base interpreter holds the port, so it may be the venv's python.exe
    # above it that names this install.
    if (Test-ProcessUnder $owner $Root) { return "ours (Valorant Overseer, PID $owner)" }
    $process = Find-ProcessById $owner
    if (-not $process) { return "unknown process (PID $owner)" }
    return "foreign: $(Split-Path -Leaf "$($process.ExecutablePath)") (PID $owner)"
}

Write-Host ""
Write-Host "  OVERSEER DIAGNOSTICS" -ForegroundColor Red

Heading "App"
$version = Get-AppVersion
Say "version: $version"
try {
    $mf = Read-Manifest
    Say "channel: $($mf.channel), supported runtime: CPython $($mf.python.version) x64, protocol: $($mf.speaks)"
    if ($mf.version -ne $version) { Flag "runtime.json says $($mf.version) but VERSION says $version" }
}
catch { Flag "runtime.json: $($_.Exception.Message)" }
$releaseFile = Join-Path $Root "release-manifest.json"
if (-not (Test-Path $releaseFile)) { Say "release commit: none (a source tree, or a release older than the manifest)" }
else {
    try { Say "release commit: $((Read-Json $releaseFile).commit)" } catch { }
}

Heading "Windows"
$os = [Environment]::OSVersion.Version
Say "windows: $($os.Major).$($os.Minor) build $($os.Build), 64-bit OS: $([Environment]::Is64BitOperatingSystem)"
# A 32-bit PowerShell sees x86 here, and the real answer is in ARCHITEW6432.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
if ($arch -eq "AMD64") { Pass "architecture: x64" } else { Flag "architecture: $arch (only x64 is supported)" }
Say "windows powershell: $($PSVersionTable.PSVersion)"
if (-not (Get-Command pwsh -ErrorAction SilentlyContinue)) { Say "powershell 7: not installed, and not needed" }
else {
    $seven = "present"
    try { $seven = & pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString()' 2>$null } catch { }
    Say "powershell 7: $seven"
}

Heading "Install Path"
$odd = @(([ordered]@{
            "spaces"                                 = $Root -match '\s'
            "special characters"                     = $Root -match "['&()%!]"
            "non-ASCII"                              = $Root -match '[^\x00-\x7F]'
            "very long path ($($Root.Length) chars)" = $Root.Length -gt 180
        }).GetEnumerator() | Where-Object { $_.Value } | ForEach-Object { $_.Key })
$kind = switch -Regex ($Root) {
    'OneDrive' { "OneDrive-synced"; break }
    '\\Downloads\\' { "Downloads"; break }
    '\.zip[\\/]|\\Temp1_' { "INSIDE A ZIP (must extract first)"; break }
    default { "normal" }
}
$why = ""
if ($odd.Count -gt 0) { $why = " ($($odd -join ', '))" }
Say "path category: $kind$why"
if ($kind -like "INSIDE*") { Flag "running from inside a ZIP" }
if (Test-Writable $Root) { Pass "install folder writable" } else { Flag "install folder isn't writable" }
if (Test-Writable $env:TEMP) { Pass "TEMP writable" } else { Flag "TEMP isn't writable" }
try {
    $free = (Get-Item $Root).PSDrive.Free
    Say ("free disk space: {0:N1} GB" -f ($free / 1GB))
    if ($free -lt 2GB) { Flag "less than 2 GB free" }
}
catch { }

Heading "Python"
try {
    $py = Find-Python
    if (-not $py) { Flag "no CPython $((Read-Manifest).python.version) x64 found on this PC (install.bat installs it)" }
    else {
        $info = Get-PythonInfo $py.Exe $py.Args
        Pass "supported python found: $($info.exe) ($($info.version) $($info.machine) $($info.bits)-bit)"
    }
}
catch { Flag "python discovery failed: $($_.Exception.Message)" }
if (-not (Test-Path $VenvPy)) { Flag ".venv is missing. Run install.bat." }
else {
    $faults = @(Get-VenvFaults)
    if ($faults.Count -eq 0) { Pass ".venv healthy (python, pip $(Get-PipVersion), packages, imports)" }
    $faults | ForEach-Object { Flag ".venv: $_" }
}
$fault = Get-MarkerFault
if ($fault) { Flag "install markers: $fault" } else { Pass "install markers valid (format $MarkerFormat)" }

Heading "Ports"
$wsPort = 7878
if (Test-Path $EnvFile) {
    $set = Select-String -Path $EnvFile -Pattern '^\s*WS_PORT\s*=\s*(\d+)' -Encoding UTF8 | Select-Object -First 1
    if ($set) { $wsPort = [int]$set.Matches[0].Groups[1].Value }
}
$stateFile = Join-Path $OverseerDir "runtime-state.json"
if (Test-Path $stateFile) {
    $state = $null
    try { $state = Read-Json $stateFile } catch { }
    $launcher = Find-ProcessById $state.pid
    $here = $Root.ToLowerInvariant() + '\'
    if (-not $launcher) { Say "runtime-state.json is stale (launcher is not running)" }
    elseif ("$($launcher.ExecutablePath) $($launcher.CommandLine)".ToLowerInvariant().Contains($here)) {
        $wsPort = [int]$state.wsPort
        Say "the running launcher picked websocket port $wsPort"
    }
    else { Say "runtime-state.json is stale (PID now belongs to another program)" }
}
Say "websocket port $wsPort`: $(Get-PortOwner $wsPort)"

Heading "Running App"
$bridge = $null
$bridgeFile = Join-Path $OverseerDir "bridge.json"
if (Test-Path $bridgeFile) {
    try { $bridge = Read-Json $bridgeFile } catch { }
}
if (-not ($bridge -and (Find-ProcessById $bridge.pid))) { Say "backend: not running (start it with start.bat)" }
elseif ((Get-PortOwner ([int]$bridge.wsPort)) -like "ours*") {
    Pass "backend bridge healthy (protocol $($bridge.protocol), ws $($bridge.wsPort), PID $($bridge.pid))"
}
else { Flag "bridge.json names port $($bridge.wsPort), but no Valorant Overseer process is listening there" }

Heading "Riot"
$lockfile = Join-Path $env:LOCALAPPDATA "Riot Games\Riot Client\Config\lockfile"
if (-not (Test-Path $lockfile)) {
    Say "Riot lockfile: not found, so the Riot Client isn't running. Live data needs the game open."
}
else {
    try {
        # The Riot Client holds it open for writing, and ReadAllText only shares
        # with other readers, so that fails whenever the client is up.
        $stream = [System.IO.File]::Open($lockfile, "Open", "Read", "ReadWrite")
        try { $null = (New-Object System.IO.StreamReader($stream)).ReadToEnd() } finally { $stream.Dispose() }
        Pass "Riot lockfile present and readable (Riot Client is running)"
    }
    catch { Flag "VG-RIOT-001 Riot lockfile exists but can't be read. Restart the Riot Client." }
}

Heading "Recent Errors (Redacted)"
$recent = @(foreach ($log in $Logs) {
        $file = Join-Path $OverseerDir "$log.log"
        if (-not (Test-Path $file)) { continue }
        Get-Content $file -Encoding UTF8 -ErrorAction SilentlyContinue |
            Where-Object { $_ -match '\bERROR\b|\bVG-[A-Z]+-\d+\b|Traceback' } |
            Select-Object -Last 5 |
            ForEach-Object { "[$log] $(Protect-OverseerText $_)" }
    })
if ($recent.Count -eq 0) { Say "none found" }
$recent | ForEach-Object { Say $_ }

if ($Bundle) {
    Heading "Support Bundle"
    $dest = $BundlePath
    if (-not $dest) {
        $stamp = [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmss")
        $dest = Join-Path ([Environment]::GetFolderPath("Desktop")) "overseer-support-$stamp.zip"
    }
    $work = Join-Path $env:TEMP ("vs-bundle-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        $clean = $script:Report | ForEach-Object { Protect-OverseerText $_ }
        Write-Text (Join-Path $work "diagnose-report.txt") ($clean -join "`r`n")
        foreach ($name in "VERSION", "runtime.json", "release-manifest.json") {
            $path = Join-Path $Root $name
            if (Test-Path $path) { Copy-Item $path (Join-Path $work $name) }
        }
        foreach ($name in "installed.json", "deps.json") {
            $path = Join-Path $OverseerDir $name
            if (Test-Path $path) {
                Write-Text (Join-Path $work $name) (Protect-OverseerText (Get-Content $path -Raw -Encoding UTF8))
            }
        }
        foreach ($log in $Logs) {
            $path = Join-Path $OverseerDir "$log.log"
            if (-not (Test-Path $path)) { continue }
            $tail = Get-Content $path -Encoding UTF8 | Select-Object -Last 400 | ForEach-Object { Protect-OverseerText $_ }
            Write-Text (Join-Path $work "$log.log") ($tail -join "`r`n")
        }
        if (Test-Path $dest) { Remove-Item -Force $dest }
        Compress-Archive -Path (Join-Path $work "*") -DestinationPath $dest
        Pass "support bundle written: $dest"
    }
    finally { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
}

Write-Host ""
if ($script:Blocked) { Warn "Diagnostics finished. Fix the [X] items above. install.bat repairs most of them." }
else { Ok "Diagnostics finished. Nothing is blocking the app." }
exit [int]$script:Blocked
