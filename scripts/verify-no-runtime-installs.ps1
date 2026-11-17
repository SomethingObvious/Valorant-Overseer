. (Join-Path $PSScriptRoot "common.ps1")

# The functions in common.ps1 that install something. A name here that
# common.ps1 doesn't define fails, so a rename can't quietly drop one.
$Installers = @("Initialize-Python", "Install-Python", "New-Venv", "Install-Packages",
    "Install-Binaries", "New-DesktopShortcut", "Register-Uninstall")

$Banned = @{
    "backend\run.py"    = @('"-m",\s*"pip"', "'-m',\s*'pip'", '"-m",\s*"venv"', "'-m',\s*'venv'",
        '"install"', "'install'", '"ci"', "'ci'", '"build"', "'build'", 'ensurepip',
        'os\.system\(', 'shell\s*=\s*True')
    "scripts\start.ps1" = @('pip install', '-m venv', 'npm\s+(install|ci)', 'run build') + $Installers
    "start.bat"         = @('pip install', '-m venv', 'npm\s+(install|ci)', 'run build') + $Installers
}

$faults = @()
foreach ($name in $Installers) {
    if (-not (Get-Command $name -CommandType Function -ErrorAction SilentlyContinue)) {
        $faults += "common.ps1 has no function named $name. Update the list in this script."
    }
}
foreach ($rel in $Banned.Keys) {
    $text = Get-Content (Join-Path $Root $rel) -Raw -Encoding UTF8
    $faults += @($Banned[$rel] | Where-Object { $text -match $_ } |
            ForEach-Object { "$rel contains a runtime install action: $_" })
}

$faults | ForEach-Object { Fail $_ }
if ($faults.Count -gt 0) { exit 1 }
Ok "startup path is install-free (validation + launch only)."
exit 0
