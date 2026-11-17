param([string]$Region = "")

. (Join-Path $PSScriptRoot "common.ps1")

$Regions = [ordered]@{ na = "North America"; eu = "Europe"; ap = "Asia Pacific"; kr = "Korea"; latam = "Latin America"; br = "Brazil" }

# Lineup clips need yt-dlp and FFmpeg. winget checks each download against the
# hash in its own catalogue. A PC without winget, or a failed download, only
# gets a warning, as the rest of the app runs fine without them.
function Install-ClipTools {
    Step "Installing what lineup clips need"
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $winget) {
        Warn "winget isn't on this PC, so yt-dlp and FFmpeg weren't installed. Lineup clips need them."
        return
    }
    $store = Join-Path $env:LOCALAPPDATA "Microsoft\WinGet"
    # Besides 0, winget's codes for "already installed" and "nothing newer".
    $fine = 0, -1978335189, -1978335135
    $packages = [ordered]@{ "yt-dlp" = "yt-dlp.yt-dlp"; "ffmpeg" = "Gyan.FFmpeg" }
    foreach ($tool in $packages.Keys) {
        # The same places the backend looks. Those are winget's links, then its
        # package folders, which a process started before they were added
        # doesn't have on PATH, and then PATH itself.
        $found = (Test-Path (Join-Path $store "Links\$tool.exe")) -or
        [bool](Get-ChildItem (Join-Path $store "Packages") -Recurse -Depth 3 -Filter "$tool.exe" -ErrorAction SilentlyContinue | Select-Object -First 1) -or
        [bool](Get-Command $tool -ErrorAction SilentlyContinue)
        if ($found) { Ok "$tool is already installed."; continue }
        & $winget.Source install --id $packages[$tool] --exact --silent --disable-interactivity `
            --accept-package-agreements --accept-source-agreements | Out-Null
        if ($LASTEXITCODE -in $fine) { Ok "$tool installed." }
        else { Warn "Couldn't install $tool (winget exit $LASTEXITCODE). Update Clip Tools in settings tries again." }
    }
}

function Read-Region {
    $codes = @($Regions.Keys)
    Step "Pick your region so the app talks to the right Riot servers:"
    for ($n = 1; $n -le $codes.Count; $n++) {
        Write-Host ("     [{0}] {1}  ({2})" -f $n, $Regions[$codes[$n - 1]], $codes[$n - 1])
    }
    $blanks = 0
    while ($true) {
        $typed = (Read-Host "  Enter a number (1-$($codes.Count))").Trim()
        if ($typed -match '^\d+$' -and [int]$typed -ge 1 -and [int]$typed -le $codes.Count) { return $codes[[int]$typed - 1] }
        if ($typed) {
            $blanks = 0
            Warn "Enter a number from 1 to $($codes.Count)."
            continue
        }
        if (++$blanks -ge 5) { throw "No region was picked. Run install.bat again and enter a number from 1 to $($codes.Count)." }
    }
}

Write-Host ""
Write-Host "  OVERSEER SETUP" -ForegroundColor Red
Write-Host "  Installs or repairs everything the app needs. Run it again any time." -ForegroundColor DarkGray

$lock = $null
$mutexes = @()
try {
    $lock = New-OverseerLock "install"
    $mutexes += New-OverseerMutex "Maintenance" "Another Valorant Overseer install or update is already running. Wait for it to finish, then try again."
    $null = Stop-RunningApp "install"
    $mutexes += New-OverseerMutex "App" "Valorant Overseer is still running and couldn't be closed automatically. Close it, then run install.bat again."
    Write-OverseerLog -Log install -Message "install/repair started (v$(Get-AppVersion))"

    Step "Checking this PC"
    $problems = @(Get-SetupProblems)
    foreach ($problem in $problems) {
        Fail $problem
        Write-OverseerLog -Log install -Level ERROR -Code VG-INSTALL-001 -Message $problem
    }
    if ($problems.Count -gt 0) { throw "This PC doesn't meet the requirements above." }
    Ok "Windows x64, writable folder, enough disk space."

    $python = Initialize-Python
    $faults = @(Get-VenvFaults)
    if ($faults.Count -eq 0) {
        Ok "The existing install is healthy, so there is nothing to reinstall."
        Write-OverseerLog -Log install -Message "venv healthy, skipping reinstall"
    }
    else {
        foreach ($fault in $faults) {
            Note "repair needed: $fault"
            Write-OverseerLog -Log install -Level WARN -Message "repair: $fault"
        }
        New-Venv $python
        Install-Packages
    }

    $saved = Get-Region
    if ($Region) {
        if (-not $Regions.Contains($Region)) { throw "Unknown region '$Region'. Use na, eu, ap, kr, latam or br." }
        Set-Region $Region
        Ok "Region saved: $Region."
    }
    elseif ($saved) { Note "Keeping the region already set ($saved). Edit backend\.env to change it." }
    elseif (Test-CanPrompt) {
        $code = Read-Region
        Set-Region $code
        Ok "Region saved: $($Regions[$code]) ($code)."
    }
    else { throw "No region is set and there is no console to ask for one. Run install.ps1 -Region na again, or use eu, ap, kr, latam or br." }

    Install-Binaries
    $retired = "discord_presence", "pick_advisor", "inventory", "match_meta", "overseer_commands" | ForEach-Object { "backend\$_.py" }
    foreach ($old in @("tui") + $retired) {
        $path = Join-Path $Root $old
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    Install-ClipTools
    New-DesktopShortcut
    Register-Uninstall
    Save-InstallMarker (Get-Region)
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
    # The app's mutex first, then maintenance, the reverse of taking them.
    [array]::Reverse($mutexes)
    $mutexes | ForEach-Object { Close-OverseerMutex $_ }
    if ($lock) { $lock.Close() }
}
