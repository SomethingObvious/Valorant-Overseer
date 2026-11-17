. (Join-Path $PSScriptRoot "common.ps1")

# What start.bat runs. It checks the install and opens the window, and never
# changes what is installed. verify-no-runtime-installs.ps1 greps this file for that.
Write-OverseerLog -Log launcher -Message "startup requested (v$(Get-AppVersion))"

$reason = Get-MarkerFault
if (-not $reason) { $reason = @(Get-VenvFaults -Quick)[0] }
if (-not $reason -and -not (Get-AppExe)) { $reason = "overseer.exe is missing" }
if ($reason) {
    Write-OverseerLog -Log launcher -Level ERROR -Code VG-DEPS-001 -Message "startup blocked: $reason"
    Show-FatalDialog "Valorant Overseer can't start: $reason.`n`nRun install.bat to repair it. Your settings and data are kept." "launcher"
    exit 1
}
Start-Process -FilePath (Get-AppExe) -WorkingDirectory $Root
