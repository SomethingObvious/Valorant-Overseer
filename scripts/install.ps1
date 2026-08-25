param(
    [string]$Region = "",
    [switch]$Frontend
)

. (Join-Path $PSScriptRoot "common.ps1")

Write-Host ""
Write-Host "  OVERSEER - SETUP" -ForegroundColor Red
Write-Host "  Installs (or repairs) everything the app needs. Safe to re-run any time." -ForegroundColor DarkGray

$lock = $null
$maintenanceMutex = $null
$appMutex = $null
try {
    $lock = New-OverseerLock "install"
    $maintenanceMutex = New-OverseerMutex "Maintenance" "Another Valorant Overseer install/update operation is already running. Wait for it to finish and retry."
    Stop-RunningApp "install" | Out-Null
    $appMutex = New-OverseerMutex "App" "Valorant Overseer is still running and couldn't be closed automatically. Close the scoreboard window, then run install.bat again."
    Write-OverseerLog -Log install -Message "install/repair started (v$(Get-LocalVersion), tree=$(if ($HasFrontend) { 'full' } else { 'slim' }))"

    Step "Checking this PC ..."
    $problems = Test-Preflight
    if ($problems.Count -gt 0) {
        foreach ($p in $problems) { Fail $p; Write-OverseerLog -Log install -Level ERROR -Code VG-INSTALL-001 -Message $p }
        throw "This PC doesn't meet the requirements above."
    }
    Ok "Windows x64, writable folder, enough disk space."

    $py = Initialize-ExactPython

    $venv = Test-Venv
    if ($venv.Ok) {
        Ok "Existing installation is healthy - nothing to reinstall."
        Write-OverseerLog -Log install -Message "venv healthy, skipping reinstall"
    }
    else {
        foreach ($r in $venv.Reasons) { Note "repair needed: $r"; Write-OverseerLog -Log install -Level WARN -Message "repair: $r" }



        Repair-Venv $py
        Install-PyDeps
    }

    if ($HasFrontend) {
        if ($Frontend) {
            if (-not (Find-Node)) { throw "Node.js 18.17+ LTS is required for the local frontend. Install it from nodejs.org and re-run with -Frontend." }
            Install-NodeDeps
            Invoke-FrontendBuild
        }
        else {
            Note "Developer tree detected - skipping frontend (run install.bat with -Frontend to build it)."
        }
    }
    else {
        Note "Slim install - no local frontend bundled; the app uses the hosted dashboard."
    }

    $saved = Get-SavedRegion
    if ($Region) {
        if ($Region -notin @("na", "eu", "ap", "kr", "latam", "br")) { throw "Unknown region '$Region'." }
        Set-Region $Region
        Ok "Region saved: $Region."
    }
    elseif ($saved) {
        Note "Region already set ($saved) - keeping it. (Edit backend\.env to change it.)"
    }
    elseif (Test-StdinInteractive) {
        $regions = @(
            @{ n = "North America"; k = "na" },
            @{ n = "Europe"; k = "eu" },
            @{ n = "Asia Pacific"; k = "ap" },
            @{ n = "Korea"; k = "kr" },
            @{ n = "Latin America"; k = "latam" },
            @{ n = "Brazil"; k = "br" }
        )
        Step "Select your region (so we talk to the right Riot servers):"
        for ($i = 0; $i -lt $regions.Count; $i++) {
            Write-Host ("     [{0}] {1}  ({2})" -f ($i + 1), $regions[$i].n, $regions[$i].k)
        }
        $choice = $null
        $blanks = 0
        while (-not $choice) {
            $r = (Read-Host "  Enter a number (1-$($regions.Count))").Trim()
            if ($r -match '^\d+$' -and [int]$r -ge 1 -and [int]$r -le $regions.Count) {
                $choice = $regions[[int]$r - 1]
            }
            elseif (-not $r) {

                if (++$blanks -ge 5) { throw "No region was selected. Run install.bat again and enter a number (1-$($regions.Count))." }
            }
            else { $blanks = 0; Warn2 "Please enter a number between 1 and $($regions.Count)." }
        }
        Set-Region $choice.k
        Ok "Region saved: $($choice.n) ($($choice.k))."
    }
    else {
        throw "No region is set and there is no console to ask. Re-run as: install.ps1 -Region na (or eu/ap/kr/latam/br)."
    }

    New-DesktopShortcut
    Save-Markers (Get-SavedRegion)
    Write-OverseerLog -Log install -Message "install/repair completed successfully"

    Write-Host ""
    Ok "Setup complete!"
    Write-Host "  Launch the app any time with start.bat (or the Valorant Overseer desktop shortcut)." -ForegroundColor Green
    exit 0
}
catch {
    Write-Host ""
    Fail "Setup failed: $($_.Exception.Message)"
    Write-OverseerLog -Log install -Level ERROR -Code VG-INSTALL-001 -Message $_.Exception.Message
    Write-Host "  Fix the issue above and run install.bat again." -ForegroundColor Yellow
    Write-Host "  (Details: $(Join-Path $OverseerDir 'install.log'))" -ForegroundColor DarkGray
    exit 1
}
finally {
    Close-OverseerMutex $appMutex
    Close-OverseerMutex $maintenanceMutex
    if ($lock) { $lock.Close() }
}
