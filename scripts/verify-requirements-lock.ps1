. (Join-Path $PSScriptRoot "common.ps1")

# Every entry in backend\requirements.txt, the ones pulled in by others too,
# has to be an exact pin followed by exactly one sha256 hash.
$faults = @()
$pins = @()
$hashed = 0
$open = $null
$code = Get-Content (Join-Path $Root "backend\requirements.txt") -Encoding UTF8 |
    ForEach-Object { ($_ -split '#', 2)[0].Trim() } |
    Where-Object { $_ }
foreach ($line in $code) {
    if ($line -match '^--hash=sha256:[0-9a-f]{64}$') {
        if ($null -eq $open) { $faults += "hash line with no pin above it: '$line'" }
        $hashed++
        $open = $null
        continue
    }
    if ($null -ne $open) { $faults += "pin has no --hash line: '$open'" }
    $pin = $line.TrimEnd('\').Trim()
    $pins += $pin
    $open = $null
    if ($pin -match '^[A-Za-z0-9._\[\]-]+==[A-Za-z0-9._+!-]+$') { $open = $pin }
    else { $faults += "not an exact pin: '$pin'" }
}
if ($null -ne $open) { $faults += "pin has no --hash line: '$open'" }
if ($pins.Count -lt 9) { $faults += "requirements.txt has only $($pins.Count) entries. Are the transitive pins missing?" }
if ($hashed -ne $pins.Count) { $faults += "$($pins.Count) pins but $hashed hashes. Every pin needs exactly one." }

$faults | ForEach-Object { Fail $_ }
if ($faults.Count -gt 0) { exit 1 }
Ok "requirements.txt: $($pins.Count) exact pins, each hash-pinned, no ranges."
exit 0
