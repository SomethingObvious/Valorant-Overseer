# Fast checks on staged content only. Anything slow belongs in pre-push.

$ErrorActionPreference = "Stop"

$root = (& git rev-parse --show-toplevel).Trim()

function Deny([string]$Reason, [string[]]$Detail) {
    Write-Host ""
    Write-Host "Commit rejected: $Reason" -ForegroundColor Red
    Write-Host ""
    foreach ($d in $Detail) { Write-Host "  $d" }
    Write-Host ""
    exit 1
}

# These two run before the skip below, because a commit under the wrong name or
# with a local file in it is public the moment it's pushed and can only be
# taken back by rewriting history.
$email = (& git config --file (Join-Path $root ".githooks\policy") --get overseer.email).Trim()
foreach ($who in "GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT") {
    $ident = (& git var $who)
    if ($ident -notmatch "<$([regex]::Escape($email))>") {
        Deny "the commit would be made as $($ident -replace '>.*$', '>')" @(
            "Every commit here is authored and committed as $email.",
            "  git config user.name SomethingObvious",
            "  git config user.email $email")
    }
}

# Anything the excludes match is meant to stay on this machine, so a tracked
# copy of it got there with git add -f.
$local = @(& git -C $root ls-files --cached --ignored --exclude-standard)
if ($local.Count -gt 0) {
    Deny "files that are excluded from the repository are staged" (
        @($local | ForEach-Object { "  $_" }) + @("Take them out and keep them on disk:  git rm -r --cached <path>"))
}

if ($env:OVERSEER_SKIP_HOOKS -eq "1") {
    Write-Host "! OVERSEER_SKIP_HOOKS is set, so the pre-commit checks were skipped" -ForegroundColor Yellow
    exit 0
}

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root "scripts\lint.ps1") -Staged
exit $LASTEXITCODE
