. (Join-Path $PSScriptRoot "common.ps1")




$bad = 0
$lines = Get-Content (Join-Path $Root "backend\requirements.txt") -Encoding UTF8
$count = 0
$hashes = 0
$pending = ""
foreach ($line in $lines) {
    $code = ($line -split '#', 2)[0].Trim()
    if (-not $code) { continue }
    if ($code -match '^--hash=sha256:[0-9a-f]{64}$') {
        if (-not $pending) { Fail "hash line with no pin above it: '$code'"; $bad = 1 }
        $hashes++
        $pending = ""
        continue
    }
    # A hashed pin ends in a backslash continuation, so a pin still pending
    # when the next one arrives never got its hash.
    if ($pending) { Fail "pin has no --hash line: '$pending'"; $bad = 1 }
    $code = $code.TrimEnd('\').Trim()
    $count++
    if ($code -notmatch '^[A-Za-z0-9._\[\]-]+==[A-Za-z0-9._+!-]+$') {
        Fail "not an exact pin: '$code'"
        $bad = 1
    }
    else { $pending = $code }
}
if ($pending) { Fail "pin has no --hash line: '$pending'"; $bad = 1 }
if ($count -lt 10) { Fail "requirements.txt has only $count entries - transitive closure missing?"; $bad = 1 }
if ($hashes -ne $count) { Fail "$count pins but $hashes hashes - every pin needs exactly one"; $bad = 1 }
if ($bad -eq 0) { Ok "requirements.txt: $count exact pins, each hash-pinned, no ranges." }
exit $bad
