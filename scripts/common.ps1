$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Windows PowerShell 5.1 can still default to TLS 1.0, which python.org refuses.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# Left set by another program, either of these points every Python started
# from here at that program's files, and with the first none of them starts.
Remove-Item Env:PYTHONHOME, Env:PYTHONPATH -ErrorAction SilentlyContinue
# pip prints the folder it runs from, and printing a letter outside this PC's
# code page raises, which a user name in another script makes it do.
$env:PYTHONIOENCODING = "utf-8:replace"

$Root = Split-Path -Parent $PSScriptRoot
$VenvDir = Join-Path $Root ".venv"
$VenvPy = Join-Path $VenvDir "Scripts\python.exe"
$OverseerDir = Join-Path $Root ".overseer"
$EnvFile = Join-Path $Root "backend\.env"
$Requirements = Join-Path $Root "backend\requirements.txt"
$MarkerFormat = 3

function Step([string]$Text) { Write-Host "`n==> $Text" -ForegroundColor Cyan }
function Ok([string]$Text) { Write-Host "  + $Text" -ForegroundColor Green }
function Note([string]$Text) { Write-Host "  . $Text" -ForegroundColor DarkGray }
function Warn([string]$Text) { Write-Host "  ! $Text" -ForegroundColor Yellow }
function Fail([string]$Text) { Write-Host "  x $Text" -ForegroundColor Red }

function Write-Text([string]$Path, [string]$Text) {
    [System.IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding($false)))
}

# Under Stop, Windows PowerShell 5.1 turns a native command's redirected stderr
# into a terminating error, so the calls that redirect it run through here.
function Invoke-Quietly([scriptblock]$Block) {
    $was = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $Block } finally { $ErrorActionPreference = $was }
}

function Read-Manifest {
    $path = Join-Path $Root "runtime.json"
    if (-not [System.IO.File]::Exists($path)) {
        throw "runtime.json is missing, so this copy is incomplete. Download the release again."
    }
    return Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Get-AppVersion {
    $path = Join-Path $Root "VERSION"
    if (-not [System.IO.File]::Exists($path)) { return "0.0.0" }
    return (Get-Content -LiteralPath $path -Raw).Trim()
}

function Get-RequirementsHash {
    if (-not (Test-Path -LiteralPath $Requirements)) { return "" }
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Requirements).Hash
}

function Test-Writable([string]$Folder) {
    $probe = Join-Path $Folder (".overseer-write-test-" + [Guid]::NewGuid().ToString("N"))
    try {
        [System.IO.File]::WriteAllText($probe, "x")
        Remove-Item -LiteralPath $probe -Force
        return $true
    }
    catch { return $false }
}

$Script:Redactions = @(
    '([?&](?:s|t|token|key)=)[^&\s"'']+', '$1[REDACTED]'
    '\b([st]=)[A-Za-z0-9._~-]{8,}', '$1[REDACTED]'
    '("(?:token|password|apiKey|api_key|key|secret|authorization)"\s*:\s*")[^"]+(")', '$1[REDACTED]$2'
    '\b(Basic|Bearer)\s+[A-Za-z0-9+/=_\-.]{8,}', '$1 [REDACTED]'
    '\b(password|token|secret|api_key|apikey|authorization)\s*[=:]\s*\S+', '$1=[REDACTED]'
    '\b[A-Za-z0-9_\-]{6,}\.[A-Za-z0-9_\-]{6,}:[A-Za-z0-9_\-]{16,}\b', '[REDACTED-KEY]'
    '\b([0-9a-fA-F]{8})[0-9a-fA-F\-]{24,}\b', '$1...[REDACTED]'
    '\b(\d{6})\d{11,}\b', '$1...[REDACTED]'
)
if ($env:USERPROFILE) {
    $Script:Redactions += ([regex]::Escape($env:USERPROFILE) -replace '\\\\', '[\\/]+'), '%USERPROFILE%'
}

function Protect-OverseerText([string]$Text) {
    for ($i = 0; $i -lt $Script:Redactions.Count; $i += 2) {
        $Text = [regex]::Replace($Text, $Script:Redactions[$i], $Script:Redactions[$i + 1], "IgnoreCase")
    }
    return $Text
}

function Write-OverseerLog {
    param([string]$Log, [string]$Level = "INFO", [string]$Code = "", [string]$Message)
    try {
        $null = New-Item -ItemType Directory -Force -Path $OverseerDir
        $file = Join-Path $OverseerDir "$Log.log"
        if ((Test-Path $file) -and (Get-Item $file).Length -gt 2MB) {
            foreach ($n in 4..1) {
                if (Test-Path "$file.$n") { Move-Item -Force "$file.$n" "$file.$($n + 1)" }
            }
            Move-Item -Force $file "$file.1"
        }
        $stamp = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
        $words = @($stamp, "[$Log]", $Level, $Code, (Protect-OverseerText $Message)) | Where-Object { $_ }
        [System.IO.File]::AppendAllText($file, ($words -join " ") + "`r`n", (New-Object System.Text.UTF8Encoding($false)))
    }
    catch { }
}

function Show-FatalDialog([string]$Message, [string]$LogName) {
    $text = "$Message`n`nDetails: $(Join-Path $OverseerDir "$LogName.log")"
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $null = [System.Windows.Forms.MessageBox]::Show($text, "Valorant Overseer", "OK", "Error")
    }
    catch { Fail $text }
}

function New-OverseerLock([string]$Name) {
    $null = New-Item -ItemType Directory -Force -Path $OverseerDir
    try { return [System.IO.File]::Open((Join-Path $OverseerDir "$Name.lock"), "OpenOrCreate", "ReadWrite", "None") }
    catch { throw "Another Valorant Overseer $Name operation is already running. Wait for it to finish and try again." }
}

# Names this install folder in the mutexes and the install marker, and has to
# come out the same as _path_fingerprint in backend\run.py. That is why only A
# to Z is lowered, on both sides.
function Get-FolderId {
    $path = [System.IO.Path]::GetFullPath($Root).TrimEnd('\')
    $lowered = [regex]::Replace($path, '[A-Z]', { param($m) $m.Value.ToLowerInvariant() })
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $digest = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($lowered)) }
    finally { $sha.Dispose() }
    return -join ($digest[0..7] | ForEach-Object { $_.ToString("X2") })
}

function New-OverseerMutex([string]$Purpose, [string]$Busy) {
    $mutex = New-Object System.Threading.Mutex($false, "Local\Overseer-$Purpose-$(Get-FolderId)")
    try { $mine = $mutex.WaitOne(0) }
    catch [System.Threading.AbandonedMutexException] { $mine = $true }
    if ($mine) { return $mutex }
    $mutex.Dispose()
    throw $Busy
}

function Close-OverseerMutex($Mutex) {
    if (-not $Mutex) { return }
    try { $Mutex.ReleaseMutex() } catch { }
    $Mutex.Dispose()
}

function Test-Alive([int]$Id) {
    return [bool](Get-CimInstance Win32_Process -Filter "ProcessId=$Id" -ErrorAction SilentlyContinue)
}

function Wait-Exit([int]$Id, [int]$Seconds) {
    $until = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ((Test-Alive $Id) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 200 }
    return -not (Test-Alive $Id)
}

# True when the process, or one of the three above it, names $Folder in its exe
# path or its command line. The venv's python.exe re-execs the base
# interpreter, which runs from outside the install, so the process doing the
# work is often only recognisable by its parent.
function Test-ProcessUnder([int]$ProcessId, [string]$Folder) {
    $prefix = $Folder.ToLowerInvariant() + '\'
    $id = $ProcessId
    foreach ($hop in 1..4) {
        if ($id -le 0) { return $false }
        $process = Get-CimInstance Win32_Process -Filter "ProcessId=$id" -ErrorAction SilentlyContinue
        if (-not $process) { return $false }
        if ("$($process.ExecutablePath) $($process.CommandLine)".ToLowerInvariant().Contains($prefix)) { return $true }
        $id = [int]$process.ParentProcessId
    }
    return $false
}

function Stop-RunningApp([string]$LogName = "launcher") {
    $stateFile = Join-Path $OverseerDir "runtime-state.json"
    $id = 0
    try { $id = [int](Get-Content $stateFile -Raw -Encoding UTF8 | ConvertFrom-Json).pid } catch { }
    if ($id -le 0) { return $false }
    if (-not (Test-Alive $id)) {
        Remove-Item $stateFile -Force -ErrorAction SilentlyContinue
        return $false
    }
    # The PID may have been reused since the app wrote it down, so only one
    # running from this install's .venv gets closed.
    if (-not (Test-ProcessUnder $id $VenvDir)) { return $false }
    Note "Closing the running Valorant Overseer (PID $id) first."
    Write-OverseerLog -Log $LogName -Message "closing running app pid=$id before maintenance"
    Invoke-Quietly { & taskkill /PID $id /T 2>&1 | Out-Null }
    if (-not (Wait-Exit $id 3)) { Invoke-Quietly { & taskkill /PID $id /T /F 2>&1 | Out-Null } }
    $null = Wait-Exit $id 5
    Remove-Item $stateFile -Force -ErrorAction SilentlyContinue
    Ok "Closed the running app."
    return $true
}

function Get-SetupProblems {
    $os = [Environment]::OSVersion.Version
    if ($os.Major -lt 10) { "Windows 10 or 11 is required (this is Windows $($os.Major).$($os.Minor))." }
    if (-not [Environment]::Is64BitOperatingSystem) { "64-bit Windows is required." }
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
    if ($arch -ne "AMD64") { "This release runs on x64 PCs only, and this one is $arch. ARM64 isn't supported yet." }
    if ($Root -match '\.zip[\\/]|\\Temp1_[^\\]*\\') {
        "This is running from inside the ZIP file. Extract it first (right-click it and pick Extract All), then run install.bat from the extracted folder."
    }
    # PowerShell reads [ ] in a -Path argument as a wildcard, and these scripts
    # hand the install folder to -Path all over the place.
    if ($Root -match '[\[\]]') {
        "The folder name has square brackets in it: '$Root'. Rename it without them, say 'overseer [1]' to 'overseer', then run install.bat again."
    }
    foreach ($folder in $Root, $env:TEMP) {
        if (-not (Test-Writable $folder)) {
            "The folder '$folder' isn't writable. Move Valorant Overseer to a folder you can write to, like Documents."
        }
    }
    try {
        $drive = (Get-Item $Root).PSDrive
        if ($null -ne $drive.Free -and $drive.Free -lt 2GB) { "Drive $($drive.Name): has less than 2 GB free. Free some space and try again." }
    }
    catch { }
}

function Test-CanPrompt {
    try { return -not [Console]::IsInputRedirected } catch { return $false }
}

function Get-PythonInfo([string]$Exe, [string[]]$Prefix = @()) {
    try {
        $said = Invoke-Quietly { & $Exe @($Prefix + (Join-Path $PSScriptRoot "python_probe.py")) 2>$null }
        if ($LASTEXITCODE -ne 0 -or -not $said) { return $null }
        return @($said)[-1] | ConvertFrom-Json
    }
    catch { return $null }
}

function Test-RightPython($Info, $Manifest) {
    # The Microsoft Store's Python keeps a copy of AppData to itself, so what
    # it writes into an install there is never seen by the window.
    return [bool]$Info -and "$($Info.base)" -notmatch '\\WindowsApps\\' -and
    $Info.kind -eq "CPython" -and $Info.version -eq $Manifest.python.version -and
    $Info.bits -eq 64 -and $Info.machine -eq "AMD64"
}

function Get-PythonChoices($Manifest) {
    $short = ($Manifest.python.version -split '\.')[0..1] -join '.'
    foreach ($name in "py", "python", "python3") {
        if (-not (Get-Command $name -ErrorAction SilentlyContinue)) { continue }
        $prefix = @()
        if ($name -eq "py") { $prefix = @("-$short") }
        @{ Exe = $name; Args = $prefix }
    }
    $keys = "HKCU:\Software\Python\PythonCore", "HKLM:\Software\Python\PythonCore", "HKLM:\Software\WOW6432Node\Python\PythonCore"
    $folders = @($keys | ForEach-Object { (Get-ItemProperty -Path "$_\$short\InstallPath" -ErrorAction SilentlyContinue).'(default)' })
    $folders += Join-Path $env:LOCALAPPDATA "Programs\Python\Python$($short -replace '\.', '')"
    foreach ($folder in ($folders | Where-Object { $_ })) {
        $exe = Join-Path $folder "python.exe"
        if (Test-Path $exe) { @{ Exe = $exe; Args = @() } }
    }
}

function Find-Python {
    $manifest = Read-Manifest
    foreach ($choice in (Get-PythonChoices $manifest)) {
        $info = Get-PythonInfo $choice.Exe $choice.Args
        $what = "$($choice.Exe) $($choice.Args -join ' ')".Trim()
        if (Test-RightPython $info $manifest) {
            Write-OverseerLog -Log install -Message "accepted python: $what is $($info.version) $($info.machine) $($info.bits)-bit at $($info.exe)"
            return $choice
        }
        $why = "does not run (Store alias or broken launcher)"
        if ($info) { $why = "$($info.kind) $($info.version) $($info.machine) $($info.bits)-bit" }
        Write-OverseerLog -Log install -Level WARN -Message "rejected python candidate ${what}: $why"
    }
    return $null
}

function Install-Python($Manifest) {
    $pin = $Manifest.python
    Step "Installing Python $($pin.version) (64-bit, from python.org)"
    # Off PATH and without the py launcher, so a Python already here still
    # answers to python and py. Find-Python finds this one by its folder.
    $setup = Join-Path $env:TEMP "overseer-python-$([Guid]::NewGuid().ToString('N')).exe"
    try {
        Note "Downloading $($pin.download)"
        Invoke-WebRequest -Uri $pin.download -OutFile $setup -TimeoutSec 600
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $setup).Hash
        if ($hash -ne $pin.sha256) { throw "The Python installer's SHA-256 is $hash, not the pinned $($pin.sha256), so it won't be run." }
        $signature = Get-AuthenticodeSignature -LiteralPath $setup
        if ($signature.Status -ne "Valid" -or $signature.SignerCertificate.Subject -ne $pin.signer) {
            throw "The Python installer's signature didn't check out (status $($signature.Status)), so it won't be run."
        }
        Ok "Installer verified by SHA-256 and Authenticode."
        $run = Start-Process -FilePath $setup -Wait -PassThru -ArgumentList "/quiet InstallAllUsers=0 PrependPath=0 Include_pip=1 Include_launcher=0"
        if ($run.ExitCode -ne 0) { throw "The Python installer failed with exit code $($run.ExitCode)." }
    }
    finally { Remove-Item -LiteralPath $setup -Force -ErrorAction SilentlyContinue }
}

function Initialize-Python {
    $manifest = Read-Manifest
    $python = Find-Python
    if (-not $python) {
        Note "No CPython $($manifest.python.version) x64 found on this PC."
        Install-Python $manifest
        $python = Find-Python
    }
    if (-not $python) {
        throw "VG-PY-001 Couldn't find or install CPython $($manifest.python.version) 64-bit. Install it yourself from $($manifest.python.download), then run install.bat again."
    }
    Ok "Python $($manifest.python.version) x64 ready."
    return $python
}

function Get-PipVersion {
    $said = Invoke-Quietly { & $VenvPy -m pip --version 2>$null }
    if ($LASTEXITCODE -eq 0 -and "$said" -match 'pip\s+(\S+)') { return $Matches[1] }
    return $null
}

function Read-InstallMarker {
    try { return Get-Content (Join-Path $OverseerDir "installed.json") -Raw -Encoding UTF8 | ConvertFrom-Json }
    catch { return $null }
}

# What is wrong with the venv, or nothing when it is fine. -Quick is the
# launcher's check, which never starts Python.
function Get-VenvFaults([switch]$Quick) {
    if (-not (Test-Path $VenvPy)) { return "no python.exe in .venv" }
    $manifest = Read-Manifest
    $venvAt = [System.IO.Path]::GetFullPath($VenvDir).TrimEnd('\')
    $faults = @()
    $cfgFile = Join-Path $VenvDir "pyvenv.cfg"
    if (-not (Test-Path $cfgFile)) { $faults += "venv has no pyvenv.cfg (incomplete or corrupt environment)" }
    else {
        try {
            $cfg = Get-Content $cfgFile -Raw -Encoding UTF8
            # pyvenv.cfg keeps the command that made the venv, folder and all,
            # which catches one copied in from somewhere else.
            if ($cfg -match '(?im)^command\s*=.*-m venv\s+(.+)$') {
                $madeFor = $Matches[1].Trim().Trim([char[]]@('"', "'"))
                if (-not [System.IO.Path]::IsPathRooted($madeFor)) { $madeFor = Join-Path $Root $madeFor }
                $madeFor = [System.IO.Path]::GetFullPath($madeFor).TrimEnd('\')
                if ($madeFor -ne $venvAt) { $faults += "venv was created for another folder ('$madeFor' vs '$venvAt')" }
            }
            # Updating or removing Python takes the venv's base interpreter
            # with it, and the venv's own python.exe then exits at once.
            if ($cfg -match '(?im)^home\s*=\s*(.+)$') {
                $base = $Matches[1].Trim()
                if (-not (Test-Path (Join-Path $base "python.exe"))) { $faults += "the Python the venv was made from is gone ($base)" }
            }
        }
        catch { $faults += "pyvenv.cfg is unreadable" }
    }
    $marker = Read-InstallMarker
    if ($marker.folder -and $marker.folder -ne (Get-FolderId)) { $faults += "installation folder moved since the venv was created" }
    $recorded = $null
    try { $recorded = (Get-Content (Join-Path $OverseerDir "deps.json") -Raw | ConvertFrom-Json).requirements } catch { }
    $stale = "requirements.txt changed since packages were installed (hash mismatch)"
    $depsStale = -not $recorded -or $recorded -ne (Get-RequirementsHash)
    if ($Quick) {
        if ($depsStale) { $faults += $stale }
        return $faults
    }

    $info = Get-PythonInfo $VenvPy
    if (-not $info) { return $faults + "venv python does not run (moved or corrupt venv)" }
    if (-not (Test-RightPython $info $manifest)) {
        $faults += "venv python is $($info.kind) $($info.version) $($info.machine) $($info.bits)-bit, need CPython $($manifest.python.version) x64"
    }
    if ($info.home -eq $info.base) { $faults += "python in .venv is not a virtual environment" }
    $prefix = ""
    if ($info.home) { $prefix = [System.IO.Path]::GetFullPath($info.home).TrimEnd('\') }
    if ($prefix -ne $venvAt) { $faults += "venv prefix is '$prefix', not '$venvAt', so the venv was moved" }
    if ($info.base -and -not (Test-Path (Join-Path $info.base "python.exe"))) { $faults += "venv base interpreter is gone ($($info.base))" }
    if ($faults) { return $faults }

    $pip = Get-PipVersion
    if ($pip -ne $manifest.pip) { $faults += "pip is '$pip' but the pin is $($manifest.pip)" }
    if ($depsStale) { $faults += $stale }
    $checks = [ordered]@{
        "pip check reports broken or missing dependencies"                 = @("-m", "pip", "check")
        "installed package versions do not exactly match requirements.txt" = @((Join-Path $PSScriptRoot "verify_installed.py"), "--requirements", $Requirements)
        "required packages fail to import"                                 = @((Join-Path $PSScriptRoot "import_smoke.py"))
    }
    foreach ($fault in $checks.Keys) {
        Invoke-Quietly { & $VenvPy @($checks[$fault]) 2>&1 | Out-Null }
        if ($LASTEXITCODE -ne 0) { $faults += $fault }
    }
    return $faults
}

function New-Venv($Python) {
    # Never anything but this install's own .venv, since it is deleted first.
    $target = [System.IO.Path]::GetFullPath($VenvDir)
    $own = [System.IO.Path]::GetFullPath((Join-Path $Root ".venv"))
    if ($target -ne $own) { throw "Won't touch '$target' because it isn't this install's .venv ($own)." }
    if (-not (Test-Path $target)) { Step "Creating the Python environment (.venv)" }
    else {
        Step "Rebuilding the Python environment (.venv)"
        try { Remove-Item -Recurse -Force -LiteralPath $target }
        catch {
            throw "Couldn't remove the old .venv ($($_.Exception.Message)). Close any running Valorant Overseer windows (and any antivirus quarantine on that folder), then run install.bat again."
        }
    }
    & $Python.Exe @($Python.Args + @("-m", "venv", $target))
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $VenvPy)) { throw "Couldn't create .venv (exit code $LASTEXITCODE)." }
}

function Install-Packages {
    $manifest = Read-Manifest
    Step "Installing the pinned Python packages"
    & $VenvPy -m pip install --quiet --no-warn-script-location "pip==$($manifest.pip)"
    if ($LASTEXITCODE -ne 0) { throw "Couldn't install the pinned pip $($manifest.pip) (check your internet connection)." }
    foreach ($round in 1..3) {
        & $VenvPy -m pip install --require-hashes -r $Requirements
        if ($LASTEXITCODE -eq 0) { break }
        if ($round -eq 3) { throw "VG-DEPS-001 pip install failed. Check your internet connection, then run install.bat again." }
        Warn "pip install failed. Trying again ($($round + 1) of 3)."
        Start-Sleep -Seconds 3
    }
    $checks = [ordered]@{
        "pip check failed after the install, so the packages don't agree with each other" = @("-m", "pip", "check")
        "installed package versions do not exactly match requirements.txt"                = @((Join-Path $PSScriptRoot "verify_installed.py"), "--requirements", $Requirements)
        "import smoke test failed after install"                                          = @((Join-Path $PSScriptRoot "import_smoke.py"))
    }
    foreach ($fault in $checks.Keys) {
        & $VenvPy @($checks[$fault])
        if ($LASTEXITCODE -ne 0) { throw "VG-DEPS-001 $fault." }
    }
    Ok "Python packages installed and verified."
}

function Save-InstallMarker([string]$Region) {
    $manifest = Read-Manifest
    $hash = Get-RequirementsHash
    $null = New-Item -ItemType Directory -Force -Path $OverseerDir
    $marker = [ordered]@{
        format       = $MarkerFormat
        version      = (Get-AppVersion)
        region       = $Region
        python       = $manifest.python.version
        pip          = $manifest.pip
        requirements = $hash
        folder       = (Get-FolderId)
        at           = [DateTime]::UtcNow.ToString("o")
    }
    Write-Text (Join-Path $OverseerDir "installed.json") ($marker | ConvertTo-Json)
    Write-Text (Join-Path $OverseerDir "deps.json") (@{ requirements = $hash } | ConvertTo-Json)
}

function Get-MarkerFault {
    if (-not (Test-Path (Join-Path $OverseerDir "installed.json"))) { return "not installed (no marker)" }
    $marker = Read-InstallMarker
    $version = Get-AppVersion
    if (-not $marker) { return "install marker is corrupt" }
    if ([int]$marker.format -ne $MarkerFormat) { return "install marker is from an older setup" }
    if ($marker.version -ne $version) { return "install marker version '$($marker.version)' does not match app version '$version'" }
    if ($marker.requirements -ne (Get-RequirementsHash)) { return "app files changed since install (requirements hash mismatch)" }
    if ($marker.folder -ne (Get-FolderId)) { return "the folder was moved since install" }
    return $null
}

function Get-Region {
    if (-not (Test-Path $EnvFile)) { return $null }
    $line = Select-String -LiteralPath $EnvFile -Pattern '^\s*RIOT_REGION\s*=\s*(.+)$' -Encoding UTF8 | Select-Object -First 1
    if (-not $line) { return $null }
    return $line.Matches[0].Groups[1].Value.Trim()
}

function Set-Region([string]$Region) {
    $kept = @()
    if (Test-Path $EnvFile) { $kept = @(Get-Content $EnvFile -Encoding UTF8 | Where-Object { $_ -notmatch '^\s*RIOT_REGION\s*=' }) }
    Write-Text $EnvFile ((($kept + "RIOT_REGION=$Region") -join "`r`n") + "`r`n")
}

function Get-AppExe {
    return @((Join-Path $Root "overseer.exe"), (Join-Path $Root "crates\target\release\overseer.exe")) |
        Where-Object { Test-Path $_ } | Select-Object -First 1
}

$Script:ReleaseForbidden = @(
    '(^|/)\.env$', '\.env\.local', '(^|/)frontend/', '(^|/)node_modules/', '(^|/)__pycache__/', '\.pyc$',
    '(^|/)\.venv/', '(^|/)\.overseer/', '(^|/)backend/data/', '(^|/)\.next/', '(^|/)\.git/', '(^|/)ops/',
    '(^|/)vendor/', '(^|/)tests/', '(^|/)\.github/', '(^|/)\.[^/]+/', 'client_id$'
)

# Windows PowerShell reads a .ps1 without a BOM in the old code page, and cmd
# loses its place in a .bat with bare LF endings.
function Get-EncodingFault([System.IO.FileInfo]$File) {
    $bytes = [System.IO.File]::ReadAllBytes($File.FullName)
    $bom = $bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF
    if ($File.Extension -eq ".ps1" -and -not $bom) { return "$($File.Name) isn't UTF-8 with a BOM." }
    if ([System.Text.Encoding]::UTF8.GetString($bytes).Replace("`r`n", "").Contains("`n")) { return "$($File.Name) has LF-only line endings." }
    return $null
}

function Install-Binaries {
    # A source tree builds the window and the wizard into crates\target\release. An
    # install runs them from beside the launcher, where a pin and a shortcut
    # can find them and a rebuild never has to fight the running app for the
    # file. Copied when the build is newer, so running setup again picks up a
    # new build.
    foreach ($name in @("overseer.exe", "overseer-setup.exe")) {
        $built = Join-Path $Root "crates\target\release\$name"
        $placed = Join-Path $Root $name
        if (-not (Test-Path $built)) { continue }
        if ((Test-Path $placed) -and ((Get-Item $placed).LastWriteTimeUtc -ge (Get-Item $built).LastWriteTimeUtc)) { continue }
        if ($name -eq "overseer.exe") {
            Get-Process -Name "overseer" -ErrorAction SilentlyContinue |
                Where-Object { $_.Path -eq $placed } |
                ForEach-Object {
                    Stop-Process -InputObject $_ -Force -ErrorAction SilentlyContinue
                    $null = $_.WaitForExit(10000)
                }
        }
        try {
            Copy-Item -Force -LiteralPath $built -Destination $placed
            Ok "Copied $name from the latest build."
        }
        catch { Warn "Couldn't place $name ($($_.Exception.Message))." }
    }
}

function Update-ShortcutIcon {
    # Windows caches a shortcut's icon by path, so a new logo at the same path
    # keeps showing the old one on the taskbar. A copy named for its contents
    # moves whenever the logo changes.
    $ico = Join-Path $Root "assets\overseer.ico"
    if (-not (Test-Path -LiteralPath $ico)) { return $null }
    $hash = (Get-FileHash -LiteralPath $ico -Algorithm SHA256).Hash.Substring(0, 12).ToLowerInvariant()
    $dir = Join-Path $Root ".overseer"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $copy = Join-Path $dir "icon-$hash.ico"
    if (-not (Test-Path -LiteralPath $copy)) {
        Get-ChildItem -LiteralPath $dir -Filter "icon-*.ico" | Remove-Item -Force
        Copy-Item -LiteralPath $ico -Destination $copy
    }
    return $copy
}

function Set-OverseerShortcut([string]$Lnk, [string]$Target, [string]$Arguments, [string]$Description) {
    # The shell's own link object, which starts from an empty file.
    # WScript.Shell's refuses a path with a letter outside this PC's code
    # page, and a user name in another script puts one in every path here.
    if (-not (Test-Path -LiteralPath $Lnk)) { [System.IO.File]::WriteAllBytes($Lnk, [byte[]]@()) }
    $shell = New-Object -ComObject Shell.Application
    $sc = $shell.Namespace((Split-Path -Parent $Lnk)).ParseName((Split-Path -Leaf $Lnk)).GetLink
    $sc.Path = $Target
    $sc.Arguments = $Arguments
    $sc.WorkingDirectory = $Root
    $ico = Update-ShortcutIcon
    if ($ico) { $sc.SetIconLocation($ico, 0) }
    $sc.Description = $Description
    $sc.Save($Lnk)
}

function New-DesktopShortcut {
    # On the desktop and in the Start menu, to the window's own exe, so
    # double-clicking opens the app and pinning it pins the window the taskbar
    # shows. Older installs also had a terminal shortcut, which goes.
    $exe = Get-AppExe
    if (-not $exe) { Warn "overseer.exe is missing, so there are no shortcuts to make."; return }
    $places = @([Environment]::GetFolderPath("Desktop"), [Environment]::GetFolderPath("Programs"))
    try {
        foreach ($dir in $places) {
            Set-OverseerShortcut -Lnk (Join-Path $dir "Valorant Overseer.lnk") -Target $exe -Arguments "" -Description "Valorant Overseer"
            $terminal = Join-Path $dir "Valorant Overseer (terminal).lnk"
            if (Test-Path $terminal) { Remove-Item -Force -LiteralPath $terminal }
        }
        Ok "Shortcuts ready on the desktop and in the Start menu. Right-click the running app on the taskbar and pick Pin to taskbar to keep it there."
    }
    catch { Warn "Couldn't create the shortcuts ($($_.Exception.Message))." }
}

# Settings > Apps lists what is in this key, under the current user so it
# needs no admin rights. Its Uninstall button runs uninstall.ps1.
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ValorantOverseer"

# What the app keeps outside its folder: where the window and setup sat on
# screen, and the offline chat's certificate, state and log.
$OutsideFolders = @(
    (Join-Path $env:APPDATA "valorant-overseer"),
    (Join-Path $env:APPDATA "Valorant Overseer setup"),
    (Join-Path $env:LOCALAPPDATA "Valorant Overseer")
)

# The command the Uninstall button runs. It is uninstall.ps1 while that is
# there. When somebody has deleted or moved the folder it can't be, so the
# command clears what the folder left behind itself: this entry, AppData and
# the shortcuts into the folder. Only single quotes inside, since all of it
# sits in one double-quoted argument.
function Get-UninstallCommand {
    $quote = { param([string]$Text) "'" + ($Text -replace "'", "''") + "'" }
    $script = & $quote (Join-Path $Root "scripts\uninstall.ps1")
    $into = & $quote ($Root.TrimEnd('\') + '\')
    $outside = ($OutsideFolders | ForEach-Object { & $quote $_ }) -join ", "
    $lines = @(
        "if (Test-Path -LiteralPath $script) { & $script; exit `$LASTEXITCODE }",
        "Remove-Item -LiteralPath $(& $quote $UninstallKey) -Recurse -Force -ErrorAction SilentlyContinue",
        "foreach (`$d in @($outside)) { if (Test-Path -LiteralPath `$d) { cmd.exe /d /c rd /s /q `$d } }",
        "`$w = New-Object -ComObject WScript.Shell",
        "foreach (`$d in [Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs')) { Get-ChildItem -LiteralPath `$d -Filter 'Valorant Overseer*.lnk' -ErrorAction SilentlyContinue | Where-Object { `$w.CreateShortcut(`$_.FullName).TargetPath.StartsWith($into, 'OrdinalIgnoreCase') } | Remove-Item -Force }",
        "Write-Host 'The Valorant Overseer folder was already gone, so this removed what it left in Windows.'",
        "`$null = Read-Host 'Press Enter to close'"
    )
    $ps = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
    return "`"$ps`" -NoProfile -ExecutionPolicy Bypass -Command `"$($lines -join '; ')`""
}

function Register-Uninstall {
    if (Test-Path -LiteralPath (Join-Path $Root ".git")) { return }
    try {
        New-Item -Path $UninstallKey -Force | Out-Null
        $values = @{
            DisplayName     = "Valorant Overseer"
            DisplayVersion  = (Get-AppVersion)
            DisplayIcon     = (Join-Path $Root "assets\overseer.ico")
            InstallLocation = $Root
            UninstallString = (Get-UninstallCommand)
        }
        foreach ($k in $values.Keys) { New-ItemProperty -Path $UninstallKey -Name $k -Value $values[$k] -PropertyType String -Force | Out-Null }
        foreach ($k in @("NoModify", "NoRepair")) { New-ItemProperty -Path $UninstallKey -Name $k -Value 1 -PropertyType DWord -Force | Out-Null }
        Ok "Listed in Settings > Apps, where it can be uninstalled."
    }
    catch { Warn "Couldn't list it in Settings > Apps ($($_.Exception.Message))." }
}

function Unregister-Uninstall {
    $entry = Get-ItemProperty -Path $UninstallKey -ErrorAction SilentlyContinue
    if (-not $entry) { return }
    $where = [string]$entry.InstallLocation
    $gone = (-not $where) -or -not (Test-Path -LiteralPath (Join-Path $where "scripts\uninstall.ps1"))
    if ($gone -or $where -eq $Root) { Remove-Item -Path $UninstallKey -Recurse -Force }
}
