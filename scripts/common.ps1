$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Windows PowerShell 5.1 can still default to TLS 1.0, which python.org refuses.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Root = Split-Path -Parent $PSScriptRoot
$VenvDir = Join-Path $Root ".venv"
$VenvPy = Join-Path $VenvDir "Scripts\python.exe"
$OverseerDir = Join-Path $Root ".overseer"
$EnvFile = Join-Path $Root "backend\.env"

$MarkerSchemaVersion = 2

function Step($m) { Write-Host ""; Write-Host "==> $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "  + $m" -ForegroundColor Green }
function Note($m) { Write-Host "  . $m" -ForegroundColor DarkGray }
function Warn($m) { Write-Host "  ! $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "  x $m" -ForegroundColor Red }

function Test-Cmd($name) { return [bool](Get-Command $name -ErrorAction SilentlyContinue) }

function Update-Path {
    $m = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $u = [Environment]::GetEnvironmentVariable("Path", "User")
    $env:Path = (@($m, $u) | Where-Object { $_ }) -join ";"
}

function Write-FileNoBom($path, $content) {
    $enc = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($path, $content, $enc)
}

function Get-RuntimeManifest {
    $p = Join-Path $Root "runtime.json"
    if (-not [System.IO.File]::Exists($p)) { throw "runtime.json is missing, so this copy is incomplete. Download the release again." }
    return (Get-Content -LiteralPath $p -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Get-LocalVersion {
    $f = Join-Path $Root "VERSION"
    if ([System.IO.File]::Exists($f)) { return ((Get-Content -LiteralPath $f -Raw).Trim()) }
    return "0.0.0"
}

function HashOf($rel) {
    $p = Join-Path $Root $rel
    if (Test-Path $p) { return (Get-FileHash -Algorithm SHA256 -Path $p).Hash }
    return ""
}

$Script:RedactionRules = @(
    @{ Pattern = '([?&](?:s|t|token|key)=)[^&\s"'']+'; Replace = '$1[REDACTED]' },
    @{ Pattern = '\b([st]=)[A-Za-z0-9._~-]{8,}'; Replace = '$1[REDACTED]' },
    @{ Pattern = '("(?:token|password|apiKey|api_key|key|secret|authorization)"\s*:\s*")[^"]+(")'; Replace = '$1[REDACTED]$2' },
    @{ Pattern = '\b(Basic|Bearer)\s+[A-Za-z0-9+/=_\-.]{8,}'; Replace = '$1 [REDACTED]' },
    @{ Pattern = '\b(password|token|secret|api_key|apikey|authorization)\s*[=:]\s*\S+'; Replace = '$1=[REDACTED]' },
    @{ Pattern = '\b[A-Za-z0-9_\-]{6,}\.[A-Za-z0-9_\-]{6,}:[A-Za-z0-9_\-]{16,}\b'; Replace = '[REDACTED-ABLY-KEY]' },
    @{ Pattern = '\b([0-9a-fA-F]{8})[0-9a-fA-F\-]{24,}\b'; Replace = '$1...[REDACTED]' },
    @{ Pattern = '\b(\d{6})\d{11,}\b'; Replace = '$1...[REDACTED]' }
)

function Protect-OverseerText([string]$text) {
    foreach ($r in $Script:RedactionRules) {
        $text = [regex]::Replace($text, $r.Pattern, $r.Replace, 'IgnoreCase')
    }
    return $text
}

$Script:LogMaxBytes = 2MB
$Script:LogBackups = 5

function Write-OverseerLog {
    param([string]$Log, [string]$Level = "INFO", [string]$Code = "", [string]$Message)
    try {
        if (-not (Test-Path $OverseerDir)) { New-Item -ItemType Directory -Path $OverseerDir | Out-Null }
        $file = Join-Path $OverseerDir "$Log.log"
        if ((Test-Path $file) -and ((Get-Item $file).Length -gt $Script:LogMaxBytes)) {
            for ($i = $Script:LogBackups - 1; $i -ge 1; $i--) {
                $src = "$file.$i"; $dst = "$file.$($i + 1)"
                if (Test-Path $src) { Move-Item -Force $src $dst }
            }
            Move-Item -Force $file "$file.1"
        }
        $ts = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
        $codePart = ""
        if ($Code) { $codePart = "$Code " }
        $line = "$ts [$Log] $Level $codePart$(Protect-OverseerText $Message)"
        [System.IO.File]::AppendAllText($file, $line + "`r`n", (New-Object System.Text.UTF8Encoding($false)))
    }
    catch { }
}

function Show-FatalDialog([string]$message, [string]$logName) {
    $full = "$message`n`nDetails: $(Join-Path $OverseerDir "$logName.log")"
    try {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show($full, "Valorant Overseer",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Error) | Out-Null
    }
    catch {
        Fail $full
    }
}

function New-OverseerLock([string]$name) {
    if (-not (Test-Path $OverseerDir)) { New-Item -ItemType Directory -Path $OverseerDir | Out-Null }
    $path = Join-Path $OverseerDir "$name.lock"
    try {
        return [System.IO.File]::Open($path, [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    }
    catch {
        throw "Another Valorant Overseer $name operation is already running. Wait for it to finish and try again."
    }
}

function Get-OverseerMutexName([string]$purpose) {
    return "Local\Overseer-$purpose-$(Get-PathFingerprint)"
}

function New-OverseerMutex([string]$purpose, [string]$busyMessage) {
    $created = $false
    $mutex = [System.Threading.Mutex]::new(
        $false, (Get-OverseerMutexName $purpose), [ref]$created)
    $acquired = $false
    try {
        $acquired = $mutex.WaitOne(0)
    }
    catch [System.Threading.AbandonedMutexException] {
        $acquired = $true
    }
    if (-not $acquired) {
        $mutex.Dispose()
        throw $busyMessage
    }
    return $mutex
}

function Close-OverseerMutex($mutex) {
    if (-not $mutex) { return }
    try { $mutex.ReleaseMutex() } catch { }
    try { $mutex.Dispose() } catch { }
}

# True when the process, or one of the three above it, names $Folder in its exe
# path or its command line. The venv's python.exe re-execs the base
# interpreter, which lives outside the install, so the process doing the work
# is often only recognisable by its parent.
function Test-ProcessUnder([int]$ProcessId, [string]$Folder) {
    $prefix = $Folder.ToLowerInvariant() + '\'
    $cur = $ProcessId
    for ($hop = 0; $hop -lt 4; $hop++) {
        if ($cur -le 0) { return $false }
        $p = Get-CimInstance Win32_Process -Filter "ProcessId=$cur" -ErrorAction SilentlyContinue
        if (-not $p) { return $false }
        if ((($p.ExecutablePath + " " + $p.CommandLine) + "").ToLowerInvariant().Contains($prefix)) { return $true }
        $cur = [int]$p.ParentProcessId
    }
    return $false
}

function Stop-RunningApp([string]$LogName = "launcher") {
    $stateFile = Join-Path $OverseerDir "runtime-state.json"
    if (-not (Test-Path $stateFile)) { return $false }
    $appPid = 0
    try { $appPid = [int]((Get-Content $stateFile -Raw -Encoding UTF8 | ConvertFrom-Json).pid) } catch { $appPid = 0 }
    if ($appPid -le 0) { return $false }

    if (-not (Get-CimInstance Win32_Process -Filter "ProcessId=$appPid" -ErrorAction SilentlyContinue)) {
        Remove-Item $stateFile -Force -ErrorAction SilentlyContinue
        return $false
    }

    # The PID may have been reused since the app wrote it down, so only kill it
    # if it runs from this install's .venv.
    if (-not (Test-ProcessUnder $appPid $VenvDir)) { return $false }

    Note "Closing the running Valorant Overseer (PID $appPid) first."
    Write-OverseerLog -Log $LogName -Message "closing running app pid=$appPid before maintenance"

    # Under Stop, Windows PowerShell 5.1 turns a native command's redirected
    # stderr into a terminating error. The other native calls here that redirect
    # it step out of Stop the same way.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & taskkill /PID $appPid /T 2>&1 | Out-Null
        $deadline = (Get-Date).AddSeconds(3)
        while ((Get-Date) -lt $deadline -and (Get-CimInstance Win32_Process -Filter "ProcessId=$appPid" -ErrorAction SilentlyContinue)) {
            Start-Sleep -Milliseconds 200
        }
        if (Get-CimInstance Win32_Process -Filter "ProcessId=$appPid" -ErrorAction SilentlyContinue) {
            & taskkill /PID $appPid /T /F 2>&1 | Out-Null
        }
    }
    finally { $ErrorActionPreference = $prevEap }

    # Wait for it to exit, so the files it holds are free for the maintenance
    # that follows.
    $deadline = (Get-Date).AddSeconds(5)
    while ((Get-Date) -lt $deadline -and (Get-CimInstance Win32_Process -Filter "ProcessId=$appPid" -ErrorAction SilentlyContinue)) {
        Start-Sleep -Milliseconds 200
    }
    Remove-Item $stateFile -Force -ErrorAction SilentlyContinue
    Ok "Closed the running app."
    return $true
}

function Test-Preflight {
    $problems = @()

    $os = [Environment]::OSVersion.Version
    if ($os.Major -lt 10) { $problems += "Windows 10 or 11 is required (this is Windows $($os.Major).$($os.Minor))." }
    if (-not [Environment]::Is64BitOperatingSystem) { $problems += "64-bit Windows is required." }
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
    if ($arch -ne "AMD64") { $problems += "This release runs on x64 PCs only, and this one is $arch. ARM64 isn't supported yet." }

    if ($Root -match '\.zip[\\/]' -or $Root -match '\\Temp1_[^\\]*\\') {
        $problems += "This is running from inside the ZIP file. Extract it first (right-click it and pick Extract All), then run install.bat from the extracted folder."
    }

    # PowerShell reads [ ] in a -Path argument as a wildcard, and these scripts
    # hand the install folder to -Path all over the place.
    if ($Root -match '[\[\]]') {
        $problems += "The folder name has square brackets in it: '$Root'. Rename it without them, say 'overseer [1]' to 'overseer', then run install.bat again."
    }

    foreach ($dir in @($Root, $env:TEMP)) {
        try {
            $t = Join-Path $dir (".vg-write-test-" + [Guid]::NewGuid().ToString("N"))
            [System.IO.File]::WriteAllText($t, "x")
            Remove-Item $t -Force
        }
        catch {
            $problems += "The folder '$dir' isn't writable. Move Valorant Overseer to a folder you can write to, like Documents."
        }
    }

    try {
        $drive = (Get-Item $Root).PSDrive
        if ($drive -and $null -ne $drive.Free -and $drive.Free -lt 2GB) {
            $problems += "Drive $($drive.Name): has less than 2 GB free. Free some space and try again."
        }
    }
    catch { }

    return $problems
}

function Test-StdinInteractive {
    try { return -not [Console]::IsInputRedirected } catch { return $false }
}

function Get-PythonIdentity([string]$exe, [string[]]$exeArgs) {
    $probe = Join-Path $PSScriptRoot "python_probe.py"
    try {
        $prevEAP = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try { $out = & $exe @($exeArgs + @($probe)) 2>$null }
        finally { $ErrorActionPreference = $prevEAP }
        if ($LASTEXITCODE -ne 0 -or -not $out) { return $null }
        return (($out | Select-Object -Last 1) | ConvertFrom-Json)
    }
    catch { return $null }
}

function Test-PythonExact($identity, $manifest) {
    if (-not $identity) { return $false }
    return ($identity.implementation -eq $manifest.python.implementation) -and
    ($identity.version -eq $manifest.python.version) -and
    ($identity.bits -eq $manifest.python.bits) -and
    ($identity.machine -eq "AMD64")
}

function Get-PythonCandidates {
    $cands = @()
    $mf = Get-RuntimeManifest
    $mm = ($mf.python.version -split '\.')[0..1] -join '.'
    if (Test-Cmd "py") { $cands += @{ Exe = "py"; Args = @("-$mm") } }
    if (Test-Cmd "python") { $cands += @{ Exe = "python"; Args = @() } }
    if (Test-Cmd "python3") { $cands += @{ Exe = "python3"; Args = @() } }
    $regRoots = @(
        "HKCU:\Software\Python\PythonCore\$mm\InstallPath",
        "HKLM:\Software\Python\PythonCore\$mm\InstallPath",
        "HKLM:\Software\WOW6432Node\Python\PythonCore\$mm\InstallPath"
    )
    foreach ($rk in $regRoots) {
        try {
            $ip = (Get-ItemProperty -Path $rk -ErrorAction Stop).'(default)'
            if ($ip) {
                $exe = Join-Path $ip "python.exe"
                if (Test-Path $exe) { $cands += @{ Exe = $exe; Args = @() } }
            }
        }
        catch { }
    }
    $default = Join-Path $env:LocalAppData ("Programs\Python\Python" + ($mm -replace '\.', '') + "\python.exe")
    if (Test-Path $default) { $cands += @{ Exe = $default; Args = @() } }
    return $cands
}

function Find-ExactPython {
    $mf = Get-RuntimeManifest
    foreach ($c in Get-PythonCandidates) {
        $id = Get-PythonIdentity $c.Exe $c.Args
        if (Test-PythonExact $id $mf) {
            Write-OverseerLog -Log install -Message "accepted python: $($c.Exe) $($c.Args -join ' ') -> $($id.version) $($id.machine) $($id.bits)-bit at $($id.executable)"
            return $c
        }
        if ($id) {
            Write-OverseerLog -Log install -Level WARN -Message "rejected python candidate $($c.Exe): $($id.implementation) $($id.version) $($id.machine) $($id.bits)-bit"
        }
        else {
            Write-OverseerLog -Log install -Level WARN -Message "rejected python candidate $($c.Exe): does not run (Store alias or broken launcher)"
        }
    }
    return $null
}

function Install-ExactPython {
    $mf = Get-RuntimeManifest
    Step "Installing Python $($mf.python.version) (64-bit, from python.org)"
    $tmp = Join-Path $env:TEMP ("vg-python-" + [Guid]::NewGuid().ToString("N") + ".exe")
    try {
        Note "Downloading $($mf.python.installerUrl)"
        Invoke-WebRequest -Uri $mf.python.installerUrl -OutFile $tmp -TimeoutSec 600
        $hash = (Get-FileHash -Algorithm SHA256 -Path $tmp).Hash
        if ($hash -ne $mf.python.installerSha256.ToUpper()) {
            throw "The Python installer's SHA-256 is $hash, not the pinned $($mf.python.installerSha256), so it won't be run."
        }
        $sig = Get-AuthenticodeSignature $tmp
        if ($sig.Status -ne "Valid" -or $sig.SignerCertificate.Subject -ne $mf.python.installerSubject) {
            throw "The Python installer's signature didn't check out (status $($sig.Status)), so it won't be run."
        }
        Ok "Installer verified by SHA-256 and Authenticode."
        $p = Start-Process -FilePath $tmp -Wait -PassThru -ArgumentList `
            "/quiet InstallAllUsers=0 PrependPath=1 Include_pip=1 Include_launcher=1"
        if ($p.ExitCode -ne 0) { throw "The Python installer failed with exit code $($p.ExitCode)." }
    }
    finally {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }
    Update-Path
}

function Initialize-ExactPython {
    $mf = Get-RuntimeManifest
    $py = Find-ExactPython
    if (-not $py) {
        Note "No CPython $($mf.python.version) x64 found on this PC."
        Install-ExactPython
        $py = Find-ExactPython
    }
    if (-not $py) {
        throw "VG-PY-001 Couldn't find or install CPython $($mf.python.version) 64-bit. Install it yourself from $($mf.python.installerUrl), then run install.bat again."
    }
    Ok "Python $($mf.python.version) x64 ready."
    return $py
}

function Assert-IsRepoVenv([string]$path) {
    $resolved = [System.IO.Path]::GetFullPath($path)
    $expected = [System.IO.Path]::GetFullPath((Join-Path $Root ".venv"))
    if ($resolved -ne $expected) {
        throw "Won't touch '$resolved' because it isn't this install's .venv ($expected)."
    }
    return $resolved
}

function Get-VenvPipVersion {
    try {
        $prevEAP = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try { $out = (& $VenvPy -m pip --version 2>$null) -join " " }
        finally { $ErrorActionPreference = $prevEAP }
        if ($LASTEXITCODE -ne 0 -or -not $out) { return $null }
        if ($out -match 'pip\s+(\S+)') { return $Matches[1] }
    }
    catch { }
    return $null
}

function Test-Venv([switch]$Quick) {
    $mf = Get-RuntimeManifest
    $reasons = @()

    if (-not (Test-Path $VenvPy)) { return @{ Ok = $false; Reasons = @("no python.exe in .venv") } }

    # pyvenv.cfg records the command that created the venv, including the folder
    # it was made for, which catches a venv copied from somewhere else.
    $cfgPath = Join-Path $VenvDir "pyvenv.cfg"
    if (-not (Test-Path $cfgPath)) {
        $reasons += "venv has no pyvenv.cfg (incomplete or corrupt environment)"
    }
    else {
        try {
            $cfg = Get-Content $cfgPath -Raw -Encoding UTF8
            if ($cfg -match '(?im)^command\s*=\s*(.+)$') {
                $command = $Matches[1].Trim()
                $marker = '-m venv '
                $at = $command.LastIndexOf($marker, [StringComparison]::OrdinalIgnoreCase)
                if ($at -ge 0) {
                    $createdAt = $command.Substring($at + $marker.Length).Trim().Trim('"').Trim("'")
                    if (-not [System.IO.Path]::IsPathRooted($createdAt)) {
                        $createdAt = Join-Path $Root $createdAt
                    }
                    $createdResolved = [System.IO.Path]::GetFullPath($createdAt).TrimEnd('\')
                    $expectedResolved = [System.IO.Path]::GetFullPath($VenvDir).TrimEnd('\')
                    if ($createdResolved -ne $expectedResolved) {
                        $reasons += "venv was created for another folder ('$createdResolved' vs '$expectedResolved')"
                    }
                }
            }
        }
        catch {
            $reasons += "pyvenv.cfg is unreadable"
        }
    }
    $installedFile = Join-Path $OverseerDir "installed.json"
    if (Test-Path $installedFile) {
        try {
            $installedMarker = Get-Content $installedFile -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($installedMarker.pathFingerprint -and
                $installedMarker.pathFingerprint -ne (Get-PathFingerprint)) {
                $reasons += "installation folder moved since the venv was created"
            }
        }
        catch { }
    }

    # The launcher's check stops here, without starting Python.
    if ($Quick) {
        $depsFileQ = Join-Path $OverseerDir "deps.json"
        $recordedQ = ""
        if (Test-Path $depsFileQ) {
            try { $recordedQ = (Get-Content $depsFileQ -Raw | ConvertFrom-Json).requirements } catch { }
        }
        if (-not $recordedQ -or $recordedQ -ne (HashOf "backend\requirements.txt")) {
            $reasons += "requirements.txt changed since packages were installed (hash mismatch)"
        }
        return @{ Ok = ($reasons.Count -eq 0); Reasons = $reasons }
    }

    $id = Get-PythonIdentity $VenvPy @()
    if (-not $id) {
        $reasons += "venv python does not run (moved or corrupt venv)"
    }
    else {
        if (-not (Test-PythonExact $id $mf)) {
            $reasons += "venv python is $($id.implementation) $($id.version) $($id.machine) $($id.bits)-bit, need CPython $($mf.python.version) x64"
        }
        if (-not $id.isVenv) { $reasons += "python in .venv is not a virtual environment" }
        $expected = [System.IO.Path]::GetFullPath($VenvDir).TrimEnd('\')
        $actual = ""
        if ($id.prefix) { $actual = [System.IO.Path]::GetFullPath($id.prefix).TrimEnd('\') }
        if ($actual -ne $expected) { $reasons += "venv prefix is '$actual', not '$expected', so the venv was moved" }
        if ($id.basePrefix -and -not (Test-Path (Join-Path $id.basePrefix "python.exe"))) {
            $reasons += "venv base interpreter is gone ($($id.basePrefix))"
        }
    }
    if ($reasons.Count -gt 0) { return @{ Ok = $false; Reasons = $reasons } }

    $pipVer = Get-VenvPipVersion
    if ($pipVer -ne $mf.pip.version) { $reasons += "pip is '$pipVer' but the pin is $($mf.pip.version)" }

    $depsFile = Join-Path $OverseerDir "deps.json"
    $recorded = ""
    if (Test-Path $depsFile) {
        try { $recorded = (Get-Content $depsFile -Raw | ConvertFrom-Json).requirements } catch { }
    }
    if (-not $recorded -or $recorded -ne (HashOf "backend\requirements.txt")) {
        $reasons += "requirements.txt changed since packages were installed (hash mismatch)"
    }

    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $VenvPy -m pip check 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { $reasons += "pip check reports broken or missing dependencies" }

        & $VenvPy (Join-Path $PSScriptRoot "verify_installed.py") `
            --requirements (Join-Path $Root "backend\requirements.txt") 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { $reasons += "installed package versions do not exactly match requirements.txt" }

        & $VenvPy (Join-Path $PSScriptRoot "import_smoke.py") 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { $reasons += "required packages fail to import" }
    }
    finally { $ErrorActionPreference = $prevEap }

    return @{ Ok = ($reasons.Count -eq 0); Reasons = $reasons }
}

function Repair-Venv($py) {
    $resolved = Assert-IsRepoVenv $VenvDir
    if (Test-Path $resolved) {
        Step "Rebuilding the Python environment (.venv)"
        try {
            Remove-Item -Recurse -Force $resolved
        }
        catch {
            throw "Couldn't remove the old .venv ($($_.Exception.Message)). Close any running Valorant Overseer windows (and any antivirus quarantine on that folder), then run install.bat again."
        }
    }
    else {
        Step "Creating the Python environment (.venv)"
    }
    & $py.Exe @($py.Args + @("-m", "venv", $resolved))
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $VenvPy)) { throw "Couldn't create .venv (exit code $LASTEXITCODE)." }
}

function Install-PyDeps {
    $mf = Get-RuntimeManifest
    Step "Installing the pinned Python packages"
    & $VenvPy -m pip install --quiet --no-warn-script-location "pip==$($mf.pip.version)"
    if ($LASTEXITCODE -ne 0) { throw "Couldn't install the pinned pip $($mf.pip.version) (check your internet connection)." }
    $req = Join-Path $Root "backend\requirements.txt"
    $done = $false
    for ($i = 1; $i -le 3; $i++) {
        if ($i -gt 1) { Warn "pip install failed. Trying again ($i of 3)."; Start-Sleep -Seconds 3 }
        & $VenvPy -m pip install --require-hashes -r $req
        if ($LASTEXITCODE -eq 0) { $done = $true; break }
    }
    if (-not $done) { throw "VG-DEPS-001 pip install failed. Check your internet connection, then run install.bat again." }

    & $VenvPy -m pip check
    if ($LASTEXITCODE -ne 0) { throw "VG-DEPS-001 pip check failed after the install, so the packages don't agree with each other." }
    & $VenvPy (Join-Path $PSScriptRoot "verify_installed.py") `
        --requirements (Join-Path $Root "backend\requirements.txt")
    if ($LASTEXITCODE -ne 0) { throw "VG-DEPS-001 installed package versions do not exactly match requirements.txt." }
    & $VenvPy (Join-Path $PSScriptRoot "import_smoke.py")
    if ($LASTEXITCODE -ne 0) { throw "VG-DEPS-001 import smoke test failed after install." }
    Ok "Python packages installed and verified."
}

# Names this install folder in the mutexes and the install marker. It has to
# match _path_fingerprint in backend\run.py, which is why only A to Z is lowered, the
# same way on both sides.
function Get-PathFingerprint {
    $normalized = [System.IO.Path]::GetFullPath($Root).TrimEnd('\')
    $chars = $normalized.ToCharArray()
    for ($i = 0; $i -lt $chars.Length; $i++) {
        $c = [int]$chars[$i]
        if ($c -ge 0x41 -and $c -le 0x5A) { $chars[$i] = [char]($c + 0x20) }
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $bytes = [System.Text.Encoding]::UTF8.GetBytes((-join $chars))
    return ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').Substring(0, 16)
}

function Save-Markers($region) {
    $mf = Get-RuntimeManifest
    if (-not (Test-Path $OverseerDir)) { New-Item -ItemType Directory -Path $OverseerDir | Out-Null }
    $installed = @{
        schemaVersion    = $MarkerSchemaVersion
        version          = (Get-LocalVersion)
        region           = $region
        python           = @{ version = $mf.python.version; arch = $mf.python.arch }
        pip              = $mf.pip.version
        requirementsHash = (HashOf "backend\requirements.txt")
        pathFingerprint  = (Get-PathFingerprint)
        installedAt      = [DateTime]::UtcNow.ToString("o")
    } | ConvertTo-Json
    Write-FileNoBom (Join-Path $OverseerDir "installed.json") $installed
    Save-DepHashes
}

function Save-DepHashes {
    if (-not (Test-Path $OverseerDir)) { New-Item -ItemType Directory -Path $OverseerDir | Out-Null }
    $deps = @{
        requirements = (HashOf "backend\requirements.txt")
    } | ConvertTo-Json
    Write-FileNoBom (Join-Path $OverseerDir "deps.json") $deps
}

function Test-Markers {
    $f = Join-Path $OverseerDir "installed.json"
    if (-not (Test-Path $f)) { return @{ Ok = $false; Reason = "not installed (no marker)" } }
    try { $m = Get-Content $f -Raw | ConvertFrom-Json } catch { return @{ Ok = $false; Reason = "install marker is corrupt" } }
    if ([int]$m.schemaVersion -ne $MarkerSchemaVersion) { return @{ Ok = $false; Reason = "install marker is from an older setup" } }
    if ($m.version -ne (Get-LocalVersion)) { return @{ Ok = $false; Reason = "install marker version '$($m.version)' does not match app version '$(Get-LocalVersion)'" } }
    if ($m.requirementsHash -ne (HashOf "backend\requirements.txt")) { return @{ Ok = $false; Reason = "app files changed since install (requirements hash mismatch)" } }
    if ($m.pathFingerprint -ne (Get-PathFingerprint)) { return @{ Ok = $false; Reason = "the folder was moved since install" } }
    return @{ Ok = $true; Marker = $m }
}

function Set-Region($region) {
    $lines = @()
    if (Test-Path $EnvFile) {
        $lines = Get-Content $EnvFile -Encoding UTF8 | Where-Object { $_ -notmatch '^\s*RIOT_REGION\s*=' }
    }
    $lines += "RIOT_REGION=$region"
    Write-FileNoBom $EnvFile (($lines -join "`r`n") + "`r`n")
}

function Get-SavedRegion {
    if (Test-Path $EnvFile) {
        $m = (Get-Content $EnvFile -Encoding UTF8 | Select-String -Pattern '^\s*RIOT_REGION\s*=\s*(.+)$')
        if ($m) { return $m.Matches[0].Groups[1].Value.Trim() }
    }
    return $null
}

function Get-AppExe {
    # The window: beside the launcher once installed, in the build tree when
    # run from source. $null when neither exists.
    foreach ($candidate in @((Join-Path $Root "overseer.exe"), (Join-Path $Root "crates\target\release\overseer.exe"))) {
        if (Test-Path $candidate) { return $candidate }
    }
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
            # An open window holds its file until it has fully exited, which
            # can take longer than any fixed pause.
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
    $ws = New-Object -ComObject WScript.Shell
    $sc = $ws.CreateShortcut($lnk)
    $sc.TargetPath = $target
    $sc.Arguments = $arguments
    $sc.WorkingDirectory = $Root
    $ico = Update-ShortcutIcon
    if ($ico) { $sc.IconLocation = "$ico,0" }
    $sc.Description = $description
    $sc.Save()
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
    # A copy of the source isn't an install, and listing it would offer to
    # delete somebody's work.
    if (Test-Path -LiteralPath (Join-Path $Root ".git")) { return }
    try {
        New-Item -Path $UninstallKey -Force | Out-Null
        $values = @{
            DisplayName     = "Valorant Overseer"
            DisplayVersion  = (Get-LocalVersion)
            DisplayIcon     = (Join-Path $Root "assets\overseer.ico")
            InstallLocation = $Root
            UninstallString = (Get-UninstallCommand)
        }
        foreach ($k in $values.Keys) { New-ItemProperty -Path $UninstallKey -Name $k -Value $values[$k] -PropertyType String -Force | Out-Null }
        # Setup itself is the repair, so Settings offers only Uninstall.
        foreach ($k in @("NoModify", "NoRepair")) { New-ItemProperty -Path $UninstallKey -Name $k -Value 1 -PropertyType DWord -Force | Out-Null }
        Ok "Listed in Settings > Apps, where it can be uninstalled."
    }
    catch { Warn "Couldn't list it in Settings > Apps ($($_.Exception.Message))." }
}

function Unregister-Uninstall {
    # This folder's entry, or one for a folder that has since moved or gone.
    # Uninstalling a copy elsewhere leaves a working install listed.
    $entry = Get-ItemProperty -Path $UninstallKey -ErrorAction SilentlyContinue
    if (-not $entry) { return }
    $where = [string]$entry.InstallLocation
    $gone = (-not $where) -or -not (Test-Path -LiteralPath (Join-Path $where "scripts\uninstall.ps1"))
    if ($gone -or $where -eq $Root) { Remove-Item -Path $UninstallKey -Recurse -Force }
}
