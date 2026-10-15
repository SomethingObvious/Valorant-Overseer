# Fast checks on staged content only. Anything slow belongs in pre-push.

$ErrorActionPreference = "Stop"

if ($env:OVERSEER_SKIP_HOOKS -eq "1") {
    Write-Host "! OVERSEER_SKIP_HOOKS is set, so the pre-commit checks were skipped" -ForegroundColor Yellow
    exit 0
}

$root = (& git rev-parse --show-toplevel).Trim()
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root "scripts\lint.ps1") -Staged
exit $LASTEXITCODE
