# Fast checks on staged content only. Anything slow belongs in pre-push.

$ErrorActionPreference = "Stop"

if ($env:OVERSEER_SKIP_HOOKS -eq "1") {
    Write-Host "! pre-commit hooks skipped via OVERSEER_SKIP_HOOKS" -ForegroundColor Yellow
    exit 0
}

$root = (& git rev-parse --show-toplevel).Trim()
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root "scripts\lint.ps1") -Staged
exit $LASTEXITCODE
