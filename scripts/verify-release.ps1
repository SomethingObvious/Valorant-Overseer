param(
    [Parameter(Mandatory = $true)][string]$Zip
)

# Checks a built release zip on its own: no forbidden files or secrets, the
# required files with the right encodings, and no comments left in the code.
. (Join-Path $PSScriptRoot "common.ps1")

if (-not (Test-Path $Zip)) { Fail "No zip at $Zip"; exit 1 }

$work = Join-Path $env:TEMP ("vg-verify-" + [Guid]::NewGuid().ToString("N"))
try {
    Step "Extracting the zip"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Zip, $work)

    $tree = $work
    if (-not (Test-Path (Join-Path $tree "VERSION"))) {
        Fail "The app files have to be at the root of the zip, and VERSION isn't there."; exit 1
    }
    $version = (Get-Content (Join-Path $tree "VERSION") -Raw).Trim()

    Step "Scanning for forbidden content"

    $forbidden = @('(^|/)\.env$', '\.env\.local', '(^|/)frontend/', '(^|/)node_modules/',
        '(^|/)__pycache__/', '\.pyc$', '(^|/)\.venv/', '(^|/)\.overseer/',
        '(^|/)backend/data/', '(^|/)\.next/', '(^|/)\.git/', '(^|/)ops/',
        '(^|/)vendor/', '(^|/)tests/', '(^|/)\.github/', '(^|/)\.claude/', 'client_id$')
    $bad = @()
    foreach ($file in (Get-ChildItem -Path $tree -Recurse -File)) {
        $rel = $file.FullName.Substring($tree.Length + 1) -replace '\\', '/'
        foreach ($fp in $forbidden) { if ($rel -match $fp) { $bad += "$rel ($fp)" } }
    }
    if ($bad.Count -gt 0) { foreach ($b in $bad) { Fail $b }; exit 1 }
    # The same scanner build-release.ps1 runs over the staged tree, so the
    # artifact is checked against one definition of a secret.
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
    (Join-Path $PSScriptRoot "scan-secrets.ps1") -Path $tree
    if ($LASTEXITCODE -ne 0) { Fail "The artifact contains a secret."; exit 1 }
    Ok "No forbidden files, secrets or personal paths."

    Step "Checking the required files and encodings"
    foreach ($req in @("install.bat", "start.bat", "VERSION",
            "runtime.json", "backend/run.py", "backend/requirements.txt",
            "backend/app.py", "scripts/common.ps1", "scripts/install.ps1",
            "scripts/start.ps1", "scripts/uninstall.ps1", "scripts/diagnose.ps1",
            "scripts/import_smoke.py", "overseer.exe", "overseer-setup.exe")) {
        if (-not (Test-Path (Join-Path $tree ($req -replace '/', '\')))) { Fail "A required file is missing: $req"; exit 1 }
    }
    # The same comparisons verify-version.ps1 makes on the source tree.
    $mf = Get-Content (Join-Path $tree "runtime.json") -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($mf.app.version -ne $version) { Fail "runtime.json app.version is '$($mf.app.version)' but VERSION is '$version'."; exit 1 }
    foreach ($file in (Get-ChildItem -Path $tree -Recurse -File -Include *.ps1, *.bat)) {
        $bytes = [System.IO.File]::ReadAllBytes($file.FullName)
        $text = [System.IO.File]::ReadAllText($file.FullName)
        if ($file.Extension -eq ".ps1" -and ($bytes.Length -lt 3 -or $bytes[0] -ne 0xEF -or $bytes[1] -ne 0xBB -or $bytes[2] -ne 0xBF)) {
            Fail "$($file.Name) isn't UTF-8 with a BOM."; exit 1
        }
        if (($text -replace "`r`n", "") -match "`n") { Fail "$($file.Name) has LF-only line endings."; exit 1 }
    }
    Ok "Required files present, encodings OK, VERSION agrees."

    Step "Checking the code is comment-free"
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $VenvPy (Join-Path $PSScriptRoot "strip_comments.py") --check $tree
        if ($LASTEXITCODE -ne 0) { Fail "Python comments or docstrings are still in the public artifact."; exit 1 }
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
        (Join-Path $PSScriptRoot "strip_script_comments.ps1") -Root $tree -Check
        if ($LASTEXITCODE -ne 0) { Fail "PowerShell or batch comments are still in the public artifact."; exit 1 }
    }
    finally { $ErrorActionPreference = $prevEap }
    Ok "The public artifact's code is comment-free."

    Write-Host ""
    Ok "Artifact verified: $Zip (v$version)"
    exit 0
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
