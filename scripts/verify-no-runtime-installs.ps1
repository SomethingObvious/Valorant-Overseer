. (Join-Path $PSScriptRoot "common.ps1")

# The startup path checks and launches and nothing else. This fails if run.py,
# start.ps1 or start.bat contains anything that installs or builds.
$bad = 0

foreach ($rel in @("backend\run.py")) {
    $c = Get-Content (Join-Path $Root $rel) -Raw -Encoding UTF8

    foreach ($pat in @('"-m",\s*"pip"', "'-m',\s*'pip'",
            '"-m",\s*"venv"', "'-m',\s*'venv'",
            '"install"', "'install'", '"ci"', "'ci'",
            '"build"', "'build'", 'ensurepip',
            'os\.system\(', 'shell\s*=\s*True')) {
        if ($c -match $pat) { Fail "$rel contains a runtime install action: $pat"; $bad = 1 }
    }
}

# The functions in common.ps1 that install something, matched by name. A name
# here that common.ps1 doesn't define fails, so a rename can't quietly take one
# out of the check.
$Installers = @("Initialize-ExactPython", "Install-ExactPython", "Repair-Venv", "Install-PyDeps",
    "Install-Binaries", "New-DesktopShortcut", "Register-Uninstall")
foreach ($name in $Installers) {
    if (-not (Get-Command $name -CommandType Function -ErrorAction SilentlyContinue)) {
        Fail "common.ps1 has no function named $name. Update the list in this script."; $bad = 1
    }
}
foreach ($rel in @("scripts\start.ps1", "start.bat")) {
    $c = Get-Content (Join-Path $Root $rel) -Raw -Encoding UTF8
    foreach ($pat in (@('pip install', '-m venv', 'npm\s+(install|ci)', 'run build') + $Installers)) {
        if ($c -match $pat) { Fail "$rel contains a runtime install action: $pat"; $bad = 1 }
    }
}

if ($bad -eq 0) { Ok "startup path is install-free (validation + launch only)." }
exit $bad
