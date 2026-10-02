param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Output,
    [switch]$AllowDirty
)

# Builds the release zip from an allowlist of tracked files and the two exes in
# crates\target\release. The staged copy is stripped of comments and scanned, and the
# zip is extracted again and checked.
. (Join-Path $PSScriptRoot "common.ps1")

Add-Type -AssemblyName System.IO.Compression.FileSystem

$AllowExact = @(
    ".gitattributes", ".gitignore", "README.md", "LICENSE.md", "VERSION",
    "runtime.json", "install.bat", "start.bat"
)
$AllowPrefix = @("assets/", "backend/", "docs/")

# Only the scripts a user actually runs. Build tooling in every user's folder
# is noise, and a map of the checks.
$AllowScripts = @(
    "scripts/common.ps1", "scripts/install.ps1", "scripts/start.ps1", "scripts/uninstall.ps1",
    "scripts/diagnose.ps1", "scripts/import_smoke.py",
    "scripts/python_probe.py", "scripts/verify_installed.py"
)

$ForbiddenPatterns = @(
    '(^|/)\.env$', '\.env\.local', '(^|/)frontend/', '(^|/)node_modules/',
    '(^|/)__pycache__/', '\.pyc$', '(^|/)\.venv/', '(^|/)\.overseer/',
    '(^|/)backend/data/', '(^|/)\.next/', '(^|/)\.git/', '(^|/)ops/',
    '(^|/)vendor/', '(^|/)tests/', '(^|/)\.github/', '(^|/)\.claude/'
)

function Stop-Build($m) { Fail $m; exit 1 }

if ((Get-LocalVersion) -ne $Version) { Stop-Build "VERSION file is '$(Get-LocalVersion)' but you asked to build '$Version'." }
$mf = Get-RuntimeManifest
if ($mf.app.version -ne $Version) { Stop-Build "runtime.json app.version is '$($mf.app.version)' but you asked to build '$Version'." }

$commit = (& git -C $Root rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $commit.Length -ne 40) { Stop-Build "Couldn't resolve the git commit." }
$dirty = [bool](& git -C $Root status --porcelain)
if ($dirty -and -not $AllowDirty) {
    Stop-Build "The working tree has uncommitted changes. Commit or stash them before a release build, or pass -AllowDirty for a local test build."
}
if ($dirty) { Warn "The working tree has uncommitted changes, so don't publish this build." }

Step "Picking tracked files from the allowlist"

$tracked = & git -C $Root ls-files --cached --others --exclude-standard
$payload = @()
foreach ($f in $tracked) {
    $take = $false
    if ($AllowExact -contains $f) { $take = $true }
    if ($AllowScripts -contains $f) { $take = $true }
    foreach ($p in $AllowPrefix) { if ($f.StartsWith($p)) { $take = $true } }
    if (-not $take) { continue }
    foreach ($fp in $ForbiddenPatterns) {
        if ($f -match $fp) { Stop-Build "An allowlisted file matches a forbidden pattern: $f ($fp)" }
    }
    $payload += $f
}
if ($payload.Count -lt 40) { Stop-Build "Only $($payload.Count) files were picked, which is too few. Check the allowlist." }
Ok "$($payload.Count) files selected."

# The window and the wizard are gitignored, so they come from crates\target\release.
# One is stale when a file cargo would rebuild it for is dated after it, the
# same date test Install-Binaries uses. Cargo.toml and Cargo.lock aren't
# counted, because cargo leaves a binary alone when the part of them it uses
# hasn't changed, and their dates would fail a build that is current.
$Binaries = [ordered]@{
    "overseer.exe"       = @("crates/overseer-app/src", "crates/overseer-app/build.rs", "crates/overseer-core/src",
        "crates/overseer-ui/src", "crates/overseer-ui/assets", "assets/overseer.ico")
    "overseer-setup.exe" = @("crates/overseer-setup/src", "crates/overseer-app/build.rs",
        "crates/overseer-ui/src", "crates/overseer-ui/assets", "assets/overseer.ico")
}
Step "Checking the window and the wizard in crates\target\release"
foreach ($name in $Binaries.Keys) {
    $exe = Join-Path $Root "crates\target\release\$name"
    if (-not (Test-Path $exe)) { Stop-Build "crates\target\release\$name is missing. Run cargo build --release in crates first." }
    $built = (Get-Item $exe).LastWriteTimeUtc
    $newer = @(& git -C $Root ls-files --cached --others --exclude-standard -- $Binaries[$name] | Where-Object {
            (Get-Item -LiteralPath (Join-Path $Root ($_ -replace '/', '\')) -ErrorAction SilentlyContinue).LastWriteTimeUtc -gt $built })
    if ($newer.Count -gt 0) {
        Stop-Build "crates\target\release\$name is older than $($newer.Count) of its source files, $($newer[0]) among them. Run cargo build --release in crates first."
    }
}
Ok "Both are newer than their sources."

$rootFolder = "overseer-v$Version"
$work = Join-Path $env:TEMP ("vg-build-" + [Guid]::NewGuid().ToString("N"))
$stage = Join-Path $work $rootFolder
New-Item -ItemType Directory -Path $stage -Force | Out-Null

try {
    foreach ($f in $payload) {
        $src = Join-Path $Root ($f -replace '/', '\')
        if (-not (Test-Path $src)) { Stop-Build "A tracked file is missing from the working tree: $f" }
        $dst = Join-Path $stage ($f -replace '/', '\')
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dst) | Out-Null
        Copy-Item -Force $src $dst
    }
    foreach ($name in $Binaries.Keys) { Copy-Item -Force (Join-Path $Root "crates\target\release\$name") (Join-Path $stage $name) }

    Step "Stripping comments and docstrings from the staged code"

    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $VenvPy (Join-Path $PSScriptRoot "strip_comments.py") $stage
        if ($LASTEXITCODE -ne 0) { Stop-Build "Stripping the Python comments failed. Nothing was built." }
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
        (Join-Path $PSScriptRoot "strip_script_comments.ps1") -Root $stage
        if ($LASTEXITCODE -ne 0) { Stop-Build "Stripping the script comments failed. Nothing was built." }

        & $VenvPy -m compileall -q $stage 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { Stop-Build "The staged Python doesn't byte-compile after stripping." }
    }
    finally { $ErrorActionPreference = $prevEap }

    Get-ChildItem -Path $stage -Recurse -Directory -Filter "__pycache__" |
        Remove-Item -Recurse -Force
    Ok "Stripped the Python, PowerShell and batch comments, and the staged Python still byte-compiles."

    Step "Scanning the staged tree for secrets, caches and developer paths"
    foreach ($file in (Get-ChildItem -Path $stage -Recurse -File)) {
        $rel = $file.FullName.Substring($stage.Length + 1) -replace '\\', '/'
        foreach ($fp in $ForbiddenPatterns) {
            if ("/$rel" -match $fp) { Stop-Build "A forbidden file is in the staged tree: $rel" }
        }
    }
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
    (Join-Path $PSScriptRoot "scan-secrets.ps1") -Path $stage
    if ($LASTEXITCODE -ne 0) { Stop-Build "The staged tree contains a secret. Nothing was built." }
    Ok "No secrets, caches or personal paths found."

    Step "Checking encodings (.ps1 is UTF-8 with a BOM and CRLF, .bat is CRLF)"
    foreach ($file in (Get-ChildItem -Path $stage -Recurse -File -Include *.ps1, *.bat)) {
        $bytes = [System.IO.File]::ReadAllBytes($file.FullName)
        $text = [System.IO.File]::ReadAllText($file.FullName)
        if ($file.Extension -eq ".ps1") {
            if ($bytes.Length -lt 3 -or $bytes[0] -ne 0xEF -or $bytes[1] -ne 0xBB -or $bytes[2] -ne 0xBF) {
                Stop-Build "$($file.Name) isn't UTF-8 with a BOM."
            }
        }
        if (($text -replace "`r`n", "") -match "`n") { Stop-Build "$($file.Name) has LF-only line endings." }
    }
    Ok "Encodings OK."

    New-Item -ItemType Directory -Force -Path $Output | Out-Null
    $zipName = "overseer-v$Version.zip"
    $zipPath = Join-Path $Output $zipName
    if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
    Step "Zipping $zipName"
    [System.IO.Compression.ZipFile]::CreateFromDirectory($stage, $zipPath,
        [System.IO.Compression.CompressionLevel]::Optimal, $false)

    Step "Extracting the zip again and checking it"
    $verifyDir = Join-Path $work "verify"
    [System.IO.Compression.ZipFile]::ExtractToDirectory($zipPath, $verifyDir)
    if ((Get-Content (Join-Path $verifyDir "VERSION") -Raw).Trim() -ne $Version) { Stop-Build "The zip's VERSION isn't $Version." }
    foreach ($req in @("backend\run.py", "start.bat", "install.bat",
            "runtime.json", "backend\app.py", "backend\requirements.txt",
            "scripts\common.ps1", "scripts\start.ps1", "overseer.exe", "overseer-setup.exe")) {
        if (-not (Test-Path (Join-Path $verifyDir $req))) { Stop-Build "The zip is missing a required file: $req" }
    }

    # The whole app in one download: the setup, then the zip, then the zip's
    # length and OVSETUP1. The setup looks for exactly this tail on itself
    # (payload.rs in overseer-setup) and unpacks what comes before it.
    $setupPath = Join-Path $Output "Valorant-Overseer-Setup.exe"
    Step "Putting the zip inside $(Split-Path -Leaf $setupPath)"
    $zipBytes = [System.IO.File]::ReadAllBytes($zipPath)
    $setupBytes = [System.IO.File]::ReadAllBytes((Join-Path $Root "crates\target\release\overseer-setup.exe"))
    $tail = [byte[]]([BitConverter]::GetBytes([uint64]$zipBytes.Length) + [System.Text.Encoding]::ASCII.GetBytes("OVSETUP1"))
    $out = [System.IO.File]::Create($setupPath)
    try {
        foreach ($part in @($setupBytes, $zipBytes, $tail)) { $out.Write($part, 0, $part.Length) }
    }
    finally { $out.Close() }
    if ((Get-Item $setupPath).Length -ne ($setupBytes.Length + $zipBytes.Length + 16)) { Stop-Build "$setupPath came out the wrong size." }
    Ok "The setup carries the app."

    Write-Host ""
    Ok "Release artifact built and verified:"
    Note "  $setupPath"
    Note "  $zipPath"
    Note "  commit $commit$(if ($dirty) { ' (uncommitted changes)' })"
    exit 0
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
