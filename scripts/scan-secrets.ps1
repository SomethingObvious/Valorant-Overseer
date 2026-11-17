param(
    [Parameter(Mandatory = $true)][string]$Path
)

# The single secret scanner. build-release.ps1 runs it over the staged tree,
# verify-release.ps1 over the extracted artifact and lint.ps1 over the working
# tree, so all three agree on what a leak looks like.
#
# Exits 1 and names the file on a hit.

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Path)) { Write-Host "  x no such path: $Path" -ForegroundColor Red; exit 2 }

$Scans = @(
    # Any BEGIN ... PRIVATE KEY, bare PKCS#8 included, since that is what openssl
    # genpkey, cryptography and ssh-keygen -m PKCS8 emit. Split so this file does
    # not match its own pattern, the same trick the canary below uses.
    @{ Name = "private key"; Pattern = ('BEGIN [A-Z0-9 ]*PRIVATE' + ' KEY') },
    @{ Name = "API key and secret pair"; Pattern = '\b[A-Za-z0-9_\-]{6,}\.[A-Za-z0-9_\-]{6,}:[A-Za-z0-9_\-]{16,}\b' },
    @{ Name = "Supabase service key"; Pattern = 'eyJ[A-Za-z0-9_\-]{30,}\.[A-Za-z0-9_\-]{30,}' },
    @{ Name = "Riot API key"; Pattern = 'RGAPI-[0-9a-fA-F-]{20,}' },
    @{ Name = "developer absolute path"; Pattern = '[A-Za-z]:\\Users\\(?!Public)[A-Za-z0-9._ -]+\\' },

    @{ Name = "canary marker"; Pattern = ('VG-CANARY' + '-SECRET') }
)

$TextExt = @(".py", ".ps1", ".psd1", ".bat", ".cmd", ".md", ".txt", ".json",
    ".example", ".yaml", ".yml", ".cfg", ".toml", ".env", ".pem", ".svg")
$TextName = @(".gitignore", ".gitattributes", "VERSION", "policy")

$bad = 0
$SelfExempt = 'scan-secrets\.ps1$'

foreach ($file in (Get-ChildItem -LiteralPath $Path -Recurse -File -Force)) {
    if ($file.FullName -match $SelfExempt) { continue }
    if ($file.FullName -match '\\(\.git|\.venv|node_modules|__pycache__|dist)\\') { continue }
    if (($TextExt -notcontains $file.Extension.ToLowerInvariant()) -and
        ($TextName -notcontains $file.Name)) { continue }
    $rel = $file.FullName.Substring($Path.TrimEnd('\').Length).TrimStart('\')
    $content = [System.IO.File]::ReadAllText($file.FullName)
    foreach ($scan in $Scans) {
        if ($content -match $scan.Pattern) {
            Write-Host "  x possible $($scan.Name) in $rel" -ForegroundColor Red
            $bad = 1
        }
    }
}

exit $bad
