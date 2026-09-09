# git calls this with up to three arguments: the message file, where the
# message came from, and a commit sha when amending. They have to be
# accepted or PowerShell refuses the call and the commit dies on a
# positional parameter error, which is not a sentence anyone should have to
# read on a commit.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$MessageFile,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs = @()
)

# Strips assistant attribution trailers out of a commit message before anything
# else reads it.
#
# Some tooling appends a co-author trailer and a session link to every message
# it writes. This repository forbids that and commit-msg rejects it, so the two
# rules fight and the commit dies on a line nobody typed. prepare-commit-msg
# runs first, so the trailer is gone before the check happens.
#
# What counts as attribution is not decided here. It comes from
# banned-patterns.txt, the same list commit-msg, pre-push and the tracked-file
# scan all read, so there is one place to edit and this file carries none of
# the words it removes.
#
# Only a line that is BOTH attribution and shaped like a trailer is removed:
# `Key: value`, or a bare URL on its own. Attribution written into the prose is
# left exactly where it is for commit-msg to reject, because a hook that
# quietly rewrites what somebody actually wrote is worse than one that says no.

$ErrorActionPreference = "Stop"

if ($env:OVERSEER_SKIP_HOOKS -eq "1") { exit 0 }
if (-not (Test-Path -LiteralPath $MessageFile)) { exit 0 }

# A merge or squash message is git's own writing, not a tool's. Leave it be.
if ($GitArgs.Count -gt 0 -and @('merge', 'squash') -contains $GitArgs[0]) { exit 0 }

$Root = (& git rev-parse --show-toplevel).Trim()
$PatternFile = Join-Path $Root ".githooks/banned-patterns.txt"
if (-not (Test-Path -LiteralPath $PatternFile)) { exit 0 }

$patterns = @(
    Get-Content -LiteralPath $PatternFile |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ -and -not $_.StartsWith("#") }
)
if ($patterns.Count -eq 0) { exit 0 }

$raw = [System.IO.File]::ReadAllText($MessageFile)
$lines = @($raw -split "`r?`n")

# `git commit --verbose` puts a scissors line and a diff below the message. Git
# discards that itself, and a diff may legitimately contain any of these words,
# so stop at the marker rather than editing somebody's patch.
$comment = (& git config --get core.commentChar)
if (-not $comment -or $comment -eq "auto") { $comment = "#" }
$scissors = [regex]::Escape($comment) + " -{6,} >8 -{6,}"
$cut = $lines.Count
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match $scissors) { $cut = $i; break }
}

# `Key: value`, or a line that is nothing but a link.
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

# Taking trailers off the end leaves the blank line that separated them.
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
Write-Host "  removed $removed attribution $word; see .githooks/prepare-commit-msg.ps1"
exit 0
