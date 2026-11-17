param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Output,
    [switch]$AllowDirty
)

. (Join-Path $PSScriptRoot "common.ps1")
Add-Type -AssemblyName System.IO.Compression.FileSystem

$Shipped = @(
    "assets/", "backend/", "docs/",
    ".gitattributes", ".gitignore", "README.md", "LICENSE.md", "VERSION", "runtime.json", "install.bat", "start.bat",
    "scripts/common.ps1", "scripts/install.ps1", "scripts/start.ps1", "scripts/uninstall.ps1", "scripts/diagnose.ps1",
    "scripts/import_smoke.py", "scripts/python_probe.py", "scripts/verify_installed.py"
)
$Required = @("backend\run.py", "backend\app.py", "backend\requirements.txt", "start.bat", "install.bat",
    "runtime.json", "scripts\common.ps1", "scripts\start.ps1", "overseer.exe", "overseer-setup.exe")

# The window and the wizard are gitignored, so they come from
# crates\target\release. One is stale when a file cargo would rebuild it for is
# dated after it, the same date test Install-Binaries uses. Cargo.toml and
# Cargo.lock aren't counted, because cargo leaves a binary alone when the part
# of them it uses hasn't changed, and their dates would fail a build that is current.
$Binaries = [ordered]@{
    "overseer.exe"       = @("crates/overseer-app/src", "crates/overseer-app/build.rs", "crates/overseer-core/src",
        "crates/overseer-ui/src", "crates/overseer-ui/assets", "assets/overseer.ico")
    "overseer-setup.exe" = @("crates/overseer-setup/src", "crates/overseer-app/build.rs",
        "crates/overseer-ui/src", "crates/overseer-ui/assets", "assets/overseer.ico")
}

function Stop-Build([string]$Message) {
    Fail $Message
    exit 1
}

function Invoke-Checked([string]$Exe, [string[]]$Arguments, [string]$Failed) {
    Invoke-Quietly { & $Exe @Arguments | Out-Host }
    if ($LASTEXITCODE -ne 0) { Stop-Build $Failed }
}

function Get-RepoFiles([string[]]$Under = @()) {
    return & git -C $Root ls-files --cached --others --exclude-standard -- @Under
}

if ((Get-AppVersion) -ne $Version) { Stop-Build "VERSION file is '$(Get-AppVersion)' but you asked to build '$Version'." }
$manifest = Read-Manifest
if ($manifest.version -ne $Version) { Stop-Build "runtime.json version is '$($manifest.version)' but you asked to build '$Version'." }

$commit = "$(& git -C $Root rev-parse HEAD)".Trim()
if ($LASTEXITCODE -ne 0 -or $commit.Length -ne 40) { Stop-Build "Couldn't resolve the git commit." }
$dirty = [bool](& git -C $Root status --porcelain)
if ($dirty -and -not $AllowDirty) {
    Stop-Build "The working tree has uncommitted changes. Commit or stash them before a release build, or pass -AllowDirty for a local test build."
}
if ($dirty) { Warn "The working tree has uncommitted changes, so don't publish this build." }

Step "Picking tracked files from the allowlist"
$payload = @(Get-RepoFiles | Where-Object {
        $file = $_
        $Shipped | Where-Object { $file -eq $_ -or ($_.EndsWith("/") -and $file.StartsWith($_)) }
    })
foreach ($file in $payload) {
    $hit = $ReleaseForbidden | Where-Object { $file -match $_ } | Select-Object -First 1
    if ($hit) { Stop-Build "An allowlisted file matches a forbidden pattern: $file ($hit)" }
}
if ($payload.Count -lt 40) { Stop-Build "Only $($payload.Count) files were picked, which is too few. Check the allowlist." }
Ok "$($payload.Count) files selected."

Step "Checking the window and the wizard in crates\target\release"
foreach ($name in $Binaries.Keys) {
    $exe = Join-Path $Root "crates\target\release\$name"
    if (-not (Test-Path -LiteralPath $exe)) { Stop-Build "crates\target\release\$name is missing. Run cargo build --release in crates first." }
    $built = (Get-Item -LiteralPath $exe).LastWriteTimeUtc
    $newer = @(Get-RepoFiles $Binaries[$name] | Where-Object {
            (Get-Item -LiteralPath (Join-Path $Root $_) -ErrorAction SilentlyContinue).LastWriteTimeUtc -gt $built
        })
    if ($newer.Count -gt 0) {
        Stop-Build "crates\target\release\$name is older than $($newer.Count) of its source files, $($newer[0]) among them. Run cargo build --release in crates first."
    }
    # Rust writes the source path of every panic location into the exe, and
    # the registry and toolchain sit under the user folder unless a
    # --remap-path-prefix rustflag rewrites them.
    $bytes = [System.IO.File]::ReadAllText($exe, [System.Text.Encoding]::GetEncoding(28591))
    if ($bytes.IndexOf($env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
        Stop-Build "crates\target\release\$name has your user folder in it. Add --remap-path-prefix to the rustflags in crates\.cargo\config.toml and run cargo build --release again."
    }
}
Ok "Both are newer than their sources and carry no user folder."

$work = Join-Path $env:TEMP ("vg-build-" + [Guid]::NewGuid().ToString("N"))
$stage = Join-Path $work "overseer-v$Version"
$null = New-Item -ItemType Directory -Force -Path $stage
try {
    foreach ($file in $payload) {
        $from = Join-Path $Root $file
        if (-not (Test-Path -LiteralPath $from)) { Stop-Build "A tracked file is missing from the working tree: $file" }
        $to = Join-Path $stage $file
        $null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to)
        Copy-Item -Force -LiteralPath $from -Destination $to
    }
    foreach ($name in $Binaries.Keys) {
        Copy-Item -Force -LiteralPath (Join-Path $Root "crates\target\release\$name") -Destination (Join-Path $stage $name)
    }

    Step "Stripping comments and docstrings from the staged code"
    Invoke-Checked -Exe $VenvPy -Arguments @((Join-Path $PSScriptRoot "strip_comments.py"), $stage) -Failed "Stripping the Python comments failed. Nothing was built."
    Invoke-Checked -Exe "powershell.exe" -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        (Join-Path $PSScriptRoot "strip_script_comments.ps1"), "-Root", $stage) -Failed "Stripping the script comments failed. Nothing was built."
    Invoke-Checked -Exe $VenvPy -Arguments @("-m", "compileall", "-q", $stage) -Failed "The staged Python doesn't byte-compile after stripping."
    Get-ChildItem -LiteralPath $stage -Recurse -Directory -Filter "__pycache__" | Remove-Item -Recurse -Force
    Ok "Stripped the Python, PowerShell and batch comments, and the staged Python still byte-compiles."

    Step "Scanning the staged tree for secrets, caches and developer paths"
    $staged = @(Get-ChildItem -LiteralPath $stage -Recurse -File)
    foreach ($file in $staged) {
        $rel = $file.FullName.Substring($stage.Length + 1) -replace '\\', '/'
        if ($ReleaseForbidden | Where-Object { "/$rel" -match $_ }) { Stop-Build "A forbidden file is in the staged tree: $rel" }
    }
    Invoke-Checked -Exe "powershell.exe" -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
        (Join-Path $PSScriptRoot "scan-secrets.ps1"), "-Path", $stage) -Failed "The staged tree contains a secret. Nothing was built."
    Ok "No secrets, caches or personal paths found."

    Step "Checking encodings (.ps1 is UTF-8 with a BOM and CRLF, .bat is CRLF)"
    $badly = $staged | Where-Object { $_.Extension -in ".ps1", ".bat" } | ForEach-Object { Get-EncodingFault $_ } |
        Select-Object -First 1
    if ($badly) { Stop-Build $badly }
    Ok "Encodings OK."

    $null = New-Item -ItemType Directory -Force -Path $Output
    $zipPath = Join-Path $Output "overseer-v$Version.zip"
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -Force -LiteralPath $zipPath }
    Step "Zipping $(Split-Path -Leaf $zipPath)"
    [System.IO.Compression.ZipFile]::CreateFromDirectory($stage, $zipPath, "Optimal", $false)

    Step "Extracting the zip again and checking it"
    $check = Join-Path $work "verify"
    [System.IO.Compression.ZipFile]::ExtractToDirectory($zipPath, $check)
    if ((Get-Content (Join-Path $check "VERSION") -Raw).Trim() -ne $Version) { Stop-Build "The zip's VERSION isn't $Version." }
    $missing = $Required | Where-Object { -not (Test-Path (Join-Path $check $_)) } | Select-Object -First 1
    if ($missing) { Stop-Build "The zip is missing a required file: $missing" }

    # The whole app in one download. The setup comes first, then the zip, then
    # the zip's length and OVSETUP1. The setup looks for exactly this tail on
    # itself (payload.rs in overseer-setup) and unpacks what comes before it.
    $setupPath = Join-Path $Output "Valorant-Overseer-Setup.exe"
    Step "Putting the zip inside $(Split-Path -Leaf $setupPath)"
    $setup = [System.IO.File]::ReadAllBytes((Join-Path $Root "crates\target\release\overseer-setup.exe"))
    $zip = [System.IO.File]::ReadAllBytes($zipPath)
    $tail = [byte[]]([BitConverter]::GetBytes([uint64]$zip.Length) + [System.Text.Encoding]::ASCII.GetBytes("OVSETUP1"))
    $stream = [System.IO.File]::Create($setupPath)
    try { foreach ($part in $setup, $zip, $tail) { $stream.Write($part, 0, $part.Length) } }
    finally { $stream.Dispose() }
    if ((Get-Item -LiteralPath $setupPath).Length -ne $setup.Length + $zip.Length + $tail.Length) { Stop-Build "$setupPath came out the wrong size." }
    Ok "The setup carries the app."

    Write-Host ""
    Ok "Release artifact built and verified:"
    Note "  $setupPath"
    Note "  $zipPath"
    $state = ""
    if ($dirty) { $state = " (uncommitted changes)" }
    Note "  commit $commit$state"
    exit 0
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
