param(
    [Parameter(Mandatory = $true)][string]$Zip
)

# Checks a built release zip by itself. Nothing in it may be forbidden or a
# secret, the files the install needs have to be there with the right
# encodings, and the code has to be comment-free.
. (Join-Path $PSScriptRoot "common.ps1")

$Needed = @("install.bat", "start.bat", "VERSION", "runtime.json", "overseer.exe", "overseer-setup.exe",
    "backend/run.py", "backend/requirements.txt", "backend/app.py", "scripts/import_smoke.py",
    "scripts/common.ps1", "scripts/install.ps1", "scripts/start.ps1", "scripts/uninstall.ps1",
    "scripts/diagnose.ps1")

function Stop-Check([string]$Message) {
    Fail $Message
    exit 1
}

function Invoke-Checker([string]$Exe, [string[]]$Arguments) {
    $was = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $Exe @Arguments | Out-Host; return $LASTEXITCODE -eq 0 }
    finally { $ErrorActionPreference = $was }
}

if (-not (Test-Path $Zip)) { Stop-Check "No zip at $Zip" }
$tree = Join-Path $env:TEMP ("vg-verify-" + [Guid]::NewGuid().ToString("N"))
try {
    Step "Extracting the zip"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Zip, $tree)
    $versionFile = Join-Path $tree "VERSION"
    if (-not (Test-Path $versionFile)) {
        Stop-Check "The app files have to be at the root of the zip, and VERSION isn't there."
    }
    $version = (Get-Content $versionFile -Raw).Trim()
    $files = @(Get-ChildItem -Path $tree -Recurse -File)

    Step "Scanning for forbidden content"
    $hits = foreach ($file in $files) {
        $rel = $file.FullName.Substring($tree.Length + 1) -replace '\\', '/'
        $ReleaseForbidden | Where-Object { $rel -match $_ } | ForEach-Object { "$rel ($_)" }
    }
    if (@($hits).Count -gt 0) { $hits | ForEach-Object { Fail $_ }; exit 1 }
    # build-release.ps1 runs the same scanner over the staged tree, so there is
    # one idea of what a secret looks like.
    if (-not (Invoke-Checker "powershell.exe" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                (Join-Path $PSScriptRoot "scan-secrets.ps1"), "-Path", $tree))) {
        Stop-Check "The artifact contains a secret."
    }
    Ok "No forbidden files, secrets or personal paths."

    Step "Checking the required files and encodings"
    $missing = $Needed | Where-Object { -not (Test-Path (Join-Path $tree ($_ -replace '/', '\'))) } |
        Select-Object -First 1
    if ($missing) { Stop-Check "A required file is missing: $missing" }
    # verify-version.ps1 makes the same comparison on the source tree.
    $manifest = Get-Content (Join-Path $tree "runtime.json") -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.version -ne $version) {
        Stop-Check "runtime.json version is '$($manifest.version)' but VERSION is '$version'."
    }
    $badly = $files | Where-Object { $_.Extension -in ".ps1", ".bat" } | ForEach-Object { Get-EncodingFault $_ } |
        Select-Object -First 1
    if ($badly) { Stop-Check $badly }
    Ok "Required files present, encodings OK, VERSION agrees."

    Step "Checking the code is comment-free"
    if (-not (Invoke-Checker $VenvPy @((Join-Path $PSScriptRoot "strip_comments.py"), "--check", $tree))) {
        Stop-Check "Python comments or docstrings are still in the public artifact."
    }
    if (-not (Invoke-Checker "powershell.exe" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                (Join-Path $PSScriptRoot "strip_script_comments.ps1"), "-Root", $tree, "-Check"))) {
        Stop-Check "PowerShell or batch comments are still in the public artifact."
    }
    Ok "The public artifact's code is comment-free."

    Write-Host ""
    Ok "Artifact verified: $Zip (v$version)"
    exit 0
}
finally {
    Remove-Item -Recurse -Force $tree -ErrorAction SilentlyContinue
}
