. (Join-Path $PSScriptRoot "common.ps1")

# What start.bat runs. It checks the install and opens the window, and never
# changes what is installed. verify-no-runtime-installs.ps1 greps this file for that.
Write-OverseerLog -Log launcher -Message "startup requested (v$(Get-LocalVersion))"

$reason = $null
$markers = Test-Markers
if (-not $markers.Ok) { $reason = $markers.Reason }
else {
    $venv = Test-Venv -Quick
    if (-not $venv.Ok) { $reason = $venv.Reasons[0] }
    elseif (-not (Get-AppExe)) { $reason = "overseer.exe is missing" }
}
if ($reason) {
    Write-OverseerLog -Log launcher -Level ERROR -Code VG-DEPS-001 -Message "startup blocked: $reason"
    Show-FatalDialog "Valorant Overseer can't start: $reason.`n`nRun install.bat to repair it. Your settings and data are kept." "launcher"
    exit 1
}
# The window starts its own backend, hidden, so this console can go.
Start-Process -FilePath (Get-AppExe) -WorkingDirectory $Root
