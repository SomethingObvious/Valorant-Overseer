param([string]$Region = "")

. (Join-Path $PSScriptRoot "common.ps1")

# Lineup clips need yt-dlp and FFmpeg. winget checks each download against the
# hash in its own catalogue, and a PC without it, or a failed download, only
# gets a warning, since the rest of the app runs fine without them.
function Install-ClipTools {
    Step "Installing what lineup clips need"
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $winget) {
        Warn "winget isn't on this PC, so yt-dlp and FFmpeg weren't installed. Lineup clips need them."
        return
    }
    # The same places the backend looks: winget's links, its package folders,
    # which aren't on PATH for a process started before they were added, and
    # then PATH itself.
    $winGetDir = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet"
    # Besides 0, winget's codes for "already installed" and "nothing newer".
    $fine = 0, -1978335189, -1978335135
    foreach ($tool in @(@{ Id = "yt-dlp.yt-dlp"; Exe = "yt-dlp" }, @{ Id = "Gyan.FFmpeg"; Exe = "ffmpeg" })) {
        $exe = "$($tool.Exe).exe"
        $there = (Test-Path (Join-Path $winGetDir "Links\$exe")) -or
        (Get-ChildItem (Join-Path $winGetDir "Packages") -Recurse -Depth 3 -Filter $exe -ErrorAction SilentlyContinue |
            Select-Object -First 1) -or
        (Get-Command $tool.Exe -ErrorAction SilentlyContinue)
        if ($there) { Ok "$($tool.Exe) is already installed."; continue }
        & $winget.Source install --id $tool.Id --exact --silent --disable-interactivity `
            --accept-package-agreements --accept-source-agreements | Out-Null
        if ($fine -contains $LASTEXITCODE) { Ok "$($tool.Exe) installed." }
        else { Warn "Couldn't install $($tool.Exe) (winget exit $LASTEXITCODE). Update Clip Tools in settings tries again." }
    }
}

Write-Host ""
Write-Host "  OVERSEER SETUP" -ForegroundColor Red
Write-Host "  Installs or repairs everything the app needs. Run it again any time." -ForegroundColor DarkGray

$lock = $null
$maintenanceMutex = $null
$appMutex = $null
try {
    $lock = New-OverseerLock "install"
    $maintenanceMutex = New-OverseerMutex "Maintenance" "Another Valorant Overseer install or update is already running. Wait for it to finish, then try again."
    Stop-RunningApp "install" | Out-Null
    $appMutex = New-OverseerMutex "App" "Valorant Overseer is still running and couldn't be closed automatically. Close it, then run install.bat again."
    Write-OverseerLog -Log install -Message "install/repair started (v$(Get-LocalVersion))"

    Step "Checking this PC"
    $problems = Test-Preflight
    if ($problems.Count -gt 0) {
        foreach ($p in $problems) { Fail $p; Write-OverseerLog -Log install -Level ERROR -Code VG-INSTALL-001 -Message $p }
        throw "This PC doesn't meet the requirements above."
    }
    Ok "Windows x64, writable folder, enough disk space."

    $py = Initialize-ExactPython

    $venv = Test-Venv
    if ($venv.Ok) {
        Ok "The existing install is healthy, so there is nothing to reinstall."
        Write-OverseerLog -Log install -Message "venv healthy, skipping reinstall"
    }
    else {
        foreach ($r in $venv.Reasons) { Note "repair needed: $r"; Write-OverseerLog -Log install -Level WARN -Message "repair: $r" }

        Repair-Venv $py
        Install-PyDeps
    }

    $saved = Get-SavedRegion
    if ($Region) {
        if ($Region -notin @("na", "eu", "ap", "kr", "latam", "br")) { throw "Unknown region '$Region'. Use na, eu, ap, kr, latam or br." }
        Set-Region $Region
        Ok "Region saved: $Region."
    }
    elseif ($saved) {
        Note "Keeping the region already set ($saved). Edit backend\.env to change it."
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
        Step "Pick your region so the app talks to the right Riot servers:"
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
                if (++$blanks -ge 5) { throw "No region was picked. Run install.bat again and enter a number from 1 to $($regions.Count)." }
            }
            else { $blanks = 0; Warn "Enter a number from 1 to $($regions.Count)." }
        }
        Set-Region $choice.k
        Ok "Region saved: $($choice.n) ($($choice.k))."
    }
    else {
        throw "No region is set and there is no console to ask for one. Run install.ps1 -Region na again, or use eu, ap, kr, latam or br."
    }

    Install-Binaries
    # What older versions shipped and this one doesn't: the terminal
    # scoreboard and the backend modules it alone used. An upgrade in place
    # leaves them.
    $gone = "discord_presence", "pick_advisor", "inventory", "match_meta", "overseer_commands"
    foreach ($old in @("tui") + ($gone | ForEach-Object { "backend\$_.py" })) {
        $path = Join-Path $Root $old
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    Install-ClipTools
    New-DesktopShortcut
    Register-Uninstall
    Save-Markers (Get-SavedRegion)
    Write-OverseerLog -Log install -Message "install/repair completed successfully"

    Write-Host ""
    Ok "Setup complete."
    Write-Host "  The shortcut opens the window." -ForegroundColor Green
    exit 0
}
catch {
    Write-Host ""
    Fail "Setup failed: $($_.Exception.Message)"
    Write-OverseerLog -Log install -Level ERROR -Code VG-INSTALL-001 -Message $_.Exception.Message
    Write-Host "  Fix the issue above and run install.bat again." -ForegroundColor Yellow
    Write-Host "  The details are in $(Join-Path $OverseerDir 'install.log')" -ForegroundColor DarkGray
    exit 1
}
finally {
    Close-OverseerMutex $appMutex
    Close-OverseerMutex $maintenanceMutex
    if ($lock) { $lock.Close() }
}
