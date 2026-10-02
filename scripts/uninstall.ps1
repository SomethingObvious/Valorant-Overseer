param(
    # Deletes your notes, lineups, settings and match history as well. Without
    # it they stay in the folder, and installing again picks them back up. Run
    # from Settings > Apps, it asks instead.
    [switch]$RemoveData
)

. (Join-Path $PSScriptRoot "common.ps1")

# Everything a release or setup puts in the folder, by name. Nothing else in
# it is touched, so a folder that also holds somebody's own files loses only
# what Overseer put there. The shipped half comes from build-release.ps1's
# allowlist, and tui is the terminal scoreboard older installs had.
$AppEntries = @(
    ".gitattributes", ".gitignore", "README.md", "LICENSE.md", "VERSION", "runtime.json",
    "install.bat", "start.bat", "overseer.exe", "overseer-setup.exe",
    "assets", "backend", "docs", "scripts", "tui", ".venv", ".overseer", "lineups", ".env"
)

# What stays unless the player says otherwise.
$DataPaths = @(".overseer", "lineups", "backend\data", "backend\.env", ".env") |
    ForEach-Object { Join-Path $Root $_ }

# Downloaded videos sit inside the lineups but are only a cache, so they go
# either way.
$CachePaths = @("lineups\.cache") | ForEach-Object { Join-Path $Root $_ }

# True for a kept path or anything inside one.
function Test-Kept([string]$Path) {
    foreach ($d in $DataPaths) {
        if ($Path -eq $d -or $Path.StartsWith($d + '\', [StringComparison]::OrdinalIgnoreCase)) { return $true }
    }
    return $false
}

function Test-HoldsKept([string]$Path) {
    foreach ($d in $DataPaths) {
        if ($d.StartsWith($Path + '\', [StringComparison]::OrdinalIgnoreCase)) { return $true }
    }
    return $false
}

# Deletes a file or a folder. rd removes a junction or a link without going
# into it, where Remove-Item -Recurse in Windows PowerShell 5.1 follows one
# and empties whatever it points at.
function Remove-Entry([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    if (-not $item) { return }
    if ($item.PSIsContainer) {
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try { & cmd.exe /d /c rd /s /q "`"$Path`"" 2>&1 | Out-Null }
        finally { $ErrorActionPreference = $prevEap }
    }
    else {
        try { $item.Attributes = [IO.FileAttributes]::Normal; $item.Delete() } catch { }
    }
}

# Removes what Overseer put under $Dir and keeps the kept paths. At the top it
# takes only the names in $Only, and below that everything. It goes into a
# folder only when the folder holds a kept path, and never into a link.
function Remove-AppFiles([string]$Dir, [string[]]$Only) {
    foreach ($item in Get-ChildItem -LiteralPath $Dir -Force) {
        if ($Only -and $Only -notcontains $item.Name) { continue }
        if (Test-Kept $item.FullName) { continue }
        $link = [bool]($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
        if ($item.PSIsContainer -and -not $link -and (Test-HoldsKept $item.FullName)) {
            Remove-AppFiles $item.FullName $null
            continue
        }
        Remove-Entry $item.FullName
    }
}

# Folders that hold more than one program's files. An install that ended up
# in one of these, from unpacking the release straight into Downloads say,
# has its files picked out by hand rather than risk somebody else's.
function Test-SharedFolder([string]$Dir) {
    $full = [IO.Path]::GetFullPath($Dir).TrimEnd('\')
    if ($full -eq [IO.Path]::GetPathRoot($full).TrimEnd('\')) { return $true }
    $shared = @(
        $env:USERPROFILE, $env:APPDATA, $env:LOCALAPPDATA, $env:TEMP, $env:SystemRoot,
        $env:ProgramFiles, ${env:ProgramFiles(x86)},
        (Join-Path $env:LOCALAPPDATA "Programs"), (Join-Path $env:USERPROFILE "Downloads"),
        [Environment]::GetFolderPath("Desktop"), [Environment]::GetFolderPath("MyDocuments")
    )
    foreach ($s in $shared) {
        if ($s -and $full -eq $s.TrimEnd('\')) { return $true }
    }
    return $false
}

# Where a shortcut points, or nothing when it can't be read.
function Get-ShortcutTarget($Shell, [string]$Lnk) {
    try { return $Shell.CreateShortcut($Lnk).TargetPath } catch { return "" }
}

# Matches a path to one of the app's own entries in this folder, wherever it
# turns up in a command line. Only those, so an install in a folder other
# things run from never touches the other things.
$AppPathPattern = "(?i)(" + (($AppEntries | ForEach-Object { [regex]::Escape((Join-Path $Root $_)) }) -join "|") + ")(?=[\\"" ]|$)"

function Remove-Shortcuts {
    # The ones that open the app here, under any name, which catches a taskbar
    # pin. Ours by name that open a folder that's gone go too, since a moved or
    # deleted install leaves those. A second install elsewhere keeps its own.
    $ws = New-Object -ComObject WScript.Shell
    $ours = @("Valorant Overseer.lnk", "Valorant Overseer (terminal).lnk")
    $pins = Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"
    $removed = 0
    foreach ($dir in @([Environment]::GetFolderPath("Desktop"), [Environment]::GetFolderPath("Programs"), $pins)) {
        if (-not (Test-Path -LiteralPath $dir)) { continue }
        foreach ($lnk in Get-ChildItem -LiteralPath $dir -Filter "*.lnk" -Force -ErrorAction SilentlyContinue) {
            $target = Get-ShortcutTarget $ws $lnk.FullName
            $here = $target -match $AppPathPattern
            $dead = ($ours -contains $lnk.Name) -and -not ($target -and (Test-Path -LiteralPath $target))
            if ($here -or $dead) {
                Remove-Item -LiteralPath $lnk.FullName -Force -ErrorAction SilentlyContinue
                $removed++
            }
        }
    }
    $plural = if ($removed -eq 1) { "" } else { "s" }
    Ok "Removed $removed shortcut$plural."
}

function Stop-AppProcesses {
    Stop-RunningApp "install" | Out-Null
    # Anything else running the app's files holds them open: the window or a
    # backend started by hand. Matched on the exe or the command
    # line, since the venv's python.exe hands over to the base interpreter
    # outside the folder, and stopped along with its children. This script and
    # whatever started it are spared.
    $self = Get-CimInstance Win32_Process -Filter "ProcessId=$PID" -ErrorAction SilentlyContinue
    $spare = @($PID, [int]$self.ParentProcessId)
    $found = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object { $spare -notcontains [int]$_.ProcessId -and "$($_.ExecutablePath) $($_.CommandLine)" -match $AppPathPattern })
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { foreach ($p in $found) { & taskkill /PID $p.ProcessId /T /F 2>&1 | Out-Null } }
    finally { $ErrorActionPreference = $prevEap }
    $deadline = (Get-Date).AddSeconds(10)
    foreach ($p in $found) {
        while ((Get-Date) -lt $deadline -and (Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue)) {
            Start-Sleep -Milliseconds 200
        }
    }
}

# What the app keeps outside its folder: where the window and setup sat on
# screen, and the offline chat's certificate, state and log.
function Remove-OutsideFiles {
    foreach ($dir in $OutsideFolders) { Remove-Entry $dir }
    Ok "Removed what it kept in AppData."
}

Write-Host ""
Write-Host "  OVERSEER UNINSTALL" -ForegroundColor Red
Write-Host "  Removes Valorant Overseer from this PC." -ForegroundColor DarkGray

$maintenanceMutex = $null
$exit = 0
try {
    $maintenanceMutex = New-OverseerMutex "Maintenance" "A Valorant Overseer install is running. Wait for it to finish, then try again."
    $checkout = Test-Path -LiteralPath (Join-Path $Root ".git")
    # Read now, since runtime.json goes with the folder.
    $python = try { (Get-RuntimeManifest).python.version } catch { "" }
    $shared = Test-SharedFolder $Root

    if (-not $RemoveData -and -not $checkout -and (Test-StdinInteractive)) {
        $answer = (Read-Host "  Delete your notes, lineups and match history too? (y/N)").Trim()
        $RemoveData = $answer -match '^(y|yes)$'
    }
    if ($RemoveData) { $DataPaths = @() }
    Write-OverseerLog -Log install -Message "uninstall started (v$(Get-LocalVersion), removeData=$RemoveData)"

    Step "Closing Valorant Overseer"
    Stop-AppProcesses
    Ok "Nothing is running from $Root."

    Step "Removing it from Windows"
    Remove-Shortcuts
    Unregister-Uninstall
    Remove-OutsideFiles

    if ($checkout) {
        # A copy of the source is somebody's work, not something setup made.
        Note "$Root is a copy of the source, so the folder stays. Delete it yourself when you're done with it."
    }
    elseif ($shared) {
        Warn "$Root is a folder other programs use too, so nothing in it was deleted. Delete Overseer's files there yourself."
    }
    else {
        Step "Deleting the app"
        # A process can't delete the folder it's standing in.
        Set-Location -LiteralPath $env:TEMP
        [Environment]::CurrentDirectory = $env:TEMP
        foreach ($cache in $CachePaths) { Remove-Entry $cache }
        Remove-AppFiles $Root $AppEntries
        $left = @(Get-ChildItem -LiteralPath $Root -Force -ErrorAction SilentlyContinue)
        if ($left.Count -eq 0) { Remove-Entry $Root }

        if (-not (Test-Path -LiteralPath $Root)) {
            Ok "Deleted $Root."
        }
        else {
            $kept = @($left | Where-Object { Test-Kept $_.FullName })
            # A folder left only because it holds kept data isn't stuck. A file
            # of the app's still in it is.
            $stuck = @($left | Where-Object { $AppEntries -contains $_.Name -and -not (Test-Kept $_.FullName) } |
                    ForEach-Object { if ($_.PSIsContainer) { Get-ChildItem -LiteralPath $_.FullName -Recurse -Force -File -ErrorAction SilentlyContinue } else { $_ } } |
                    Where-Object { -not (Test-Kept $_.FullName) })
            $foreign = @($left | Where-Object { $AppEntries -notcontains $_.Name })
            if ($kept.Count -gt 0) { Ok "Deleted the app. Your notes, lineups and match history are still in $Root." }
            if ($stuck.Count -gt 0) { Warn "Some files were in use and are still there. Restart, then run the uninstall again." }
            if ($foreign.Count -gt 0) {
                $names = ($foreign | Select-Object -First 5 | ForEach-Object Name) -join ", "
                Note "Left what Overseer didn't put in $Root`: $names."
            }
        }
    }

    if ($python) { Note "Python $python stays, since other programs can use it. Remove it from Settings > Apps if nothing else needs it." }
    Write-Host ""
    Ok "Valorant Overseer is uninstalled."
}
catch {
    Write-Host ""
    Fail "Uninstall failed: $($_.Exception.Message)"
    Write-OverseerLog -Log install -Level ERROR -Message "uninstall failed: $($_.Exception.Message)"
    $exit = 1
}
finally {
    Close-OverseerMutex $maintenanceMutex
}

if (Test-StdinInteractive) { $null = Read-Host "  Press Enter to close" }
exit $exit
