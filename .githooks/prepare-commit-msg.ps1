# git calls this with up to three arguments: the message file, where the
# message came from, and a commit sha when amending. All three have to be
# accepted, or PowerShell refuses the call and the commit dies on a positional
# parameter error.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$MessageFile,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs = @()
)

$ErrorActionPreference = "Stop"

if ($env:OVERSEER_SKIP_HOOKS -eq "1") { exit 0 }
if (-not (Test-Path -LiteralPath $MessageFile)) { exit 0 }

if ($GitArgs.Count -gt 0 -and @('merge', 'squash') -contains $GitArgs[0]) { exit 0 }

$PatternFile = Join-Path (& git rev-parse --absolute-git-dir).Trim() "info\banned-patterns.txt"
if (-not (Test-Path -LiteralPath $PatternFile)) { exit 0 }

$patterns = @(
    Get-Content -LiteralPath $PatternFile |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ -and -not $_.StartsWith("#") }
)
if ($patterns.Count -eq 0) { exit 0 }

$raw = [System.IO.File]::ReadAllText($MessageFile)
$lines = @($raw -split "`r?`n")

$comment = (& git config --get core.commentChar)
if (-not $comment -or $comment -eq "auto") { $comment = "#" }
$scissors = [regex]::Escape($comment) + " -{6,} >8 -{6,}"
$cut = $lines.Count
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match $scissors) { $cut = $i; break }
}

$trailerShape = "^\s*[A-Za-z][A-Za-z0-9-]*:\s*\S"
$bareUrl = "^\s*https?://\S+\s*$"

$head = @()
$removed = 0
for ($i = 0; $i -lt $cut; $i++) {
    $line = $lines[$i]
    $shaped = ($line -match $trailerShape) -or ($line -match $bareUrl)
    $attributed = $false
    if ($shaped) {
        foreach ($pattern in $patterns) {
            if ($line -imatch $pattern) { $attributed = $true; break }
        }
    }
    if ($attributed) { $removed++ } else { $head += $line }
}

if ($removed -eq 0) { exit 0 }

while ($head.Count -gt 0 -and [string]::IsNullOrWhiteSpace($head[$head.Count - 1])) {
    $head = @($head[0..($head.Count - 2)])
}

$tail = @()
if ($cut -lt $lines.Count) { $tail = @($lines[$cut..($lines.Count - 1)]) }
$out = (@($head) + $tail) -join "`n"
if (-not $out.EndsWith("`n")) { $out = $out + "`n" }
[System.IO.File]::WriteAllText($MessageFile, $out)

$word = "lines"
if ($removed -eq 1) { $word = "line" }
Write-Host "  Removed $removed attribution $word. .githooks/prepare-commit-msg.ps1 says why."
exit 0
