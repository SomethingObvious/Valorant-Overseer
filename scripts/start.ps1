. (Join-Path $PSScriptRoot "common.ps1")










Start-OverseerConsole

$Script:PhaseTotal = 3
function Show-Phase([int]$step, [string]$text) {
    $width = 14
    $filled = [int][Math]::Round($width * ($step / $Script:PhaseTotal))
    if ($filled -gt $width) { $filled = $width }
    $on = "$([char]0x25B0)" * $filled
    $off = "$([char]0x25B1)" * ($width - $filled)

    Write-Host "`r " -NoNewline
    Write-Host $on -NoNewline -ForegroundColor Red
    Write-Host $off -NoNewline -ForegroundColor DarkGray
    Write-Host ("  " + $text.PadRight(52)) -NoNewline -ForegroundColor Gray
}
function Complete-Progress([string]$text) {
    Show-Phase $Script:PhaseTotal $text
    Write-Host ""
}

# Same wordmark, same indent, same colours as the scoreboard header. This
# console becomes the scoreboard a second later, so anything else reads as the
# title moving.
Write-Host ""
Write-Host " VALORANT" -NoNewline -ForegroundColor Red
Write-Host " OVERSEER" -ForegroundColor White
Write-Host ""

Write-OverseerLog -Log launcher -Message "startup requested (v$(Get-LocalVersion))"


Show-Phase 1 "Checking your installation."
$markers = Test-Markers
if (-not $markers.Ok) {
    Write-Host ""
    Write-OverseerLog -Log launcher -Level ERROR -Code VG-DEPS-001 -Message "startup blocked: $($markers.Reason)"
    Stop-OverseerConsole
    Show-FatalDialog "Valorant Overseer can't start: $($markers.Reason).`n`nRun install.bat to repair (your settings and data are kept)." "launcher"
    exit 1
}
$venv = Test-Venv -Quick
if (-not $venv.Ok) {
    Write-Host ""
    $code = "VG-DEPS-001"
    foreach ($r in $venv.Reasons) {
        if ($r -match 'python|venv') { $code = "VG-PY-001" }
        Write-OverseerLog -Log launcher -Level ERROR -Code $code -Message "startup blocked: $r"
    }
    Stop-OverseerConsole
    Show-FatalDialog "Valorant Overseer can't start: $($venv.Reasons[0]).`n`nRun install.bat to repair (your settings and data are kept)." "launcher"
    exit 1
}






Show-Phase 2 "Starting the backend."
Stop-RunningApp "launcher" | Out-Null







Complete-Progress "Opening the scoreboard."

# This console becomes the scoreboard, and the scoreboard draws its own header.
# Without this the launcher's wordmark and progress bar stay on screen above it
# and you get the title twice. 2J clears the screen, 3J clears the scrollback so
# no copy survives above the fold, H puts the cursor home.
$esc = [char]27
Write-Host "$esc[2J$esc[3J$esc[H" -NoNewline

# Which front end this install asked for. The window unless the marker says
# otherwise, and --cli always opens the terminal one, so a person who picked
# the window can still get a terminal without reinstalling.
$frontend = "both"
try {
    $marker = Join-Path $OverseerDir "installed.json"
    if (Test-Path $marker) {
        $saved = (Get-Content -Raw -LiteralPath $marker | ConvertFrom-Json).profile
        if ($saved) { $frontend = $saved }
    }
}
catch { }
$wantCli = ($args -contains "--cli") -or ($frontend -eq "cli")
$appExe = Join-Path $Root "overseer.exe"

if (-not $wantCli -and (Test-Path $appExe)) {
    Write-OverseerLog -Log launcher -Message "opening the window ($frontend)"
    Stop-OverseerConsole
    & $VenvPy (Join-Path $Root "run.py") --prod --window
    $code = $LASTEXITCODE
}
else {
    $env:VS_PREVALIDATED = "1"
    $env:VS_ATTACHED_CLI = "1"
    Write-OverseerLog -Log launcher -Message "handing this console to run.py (attached single-window mode)"
    try {
        & $VenvPy (Join-Path $Root "run.py") --prod
        $code = $LASTEXITCODE
    }
    finally {
        Stop-OverseerConsole
    }
}
Write-OverseerLog -Log launcher -Message "run.py exited with code $code"
exit $code
