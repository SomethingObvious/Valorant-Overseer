. (Join-Path $PSScriptRoot "common.ps1")

$version = Get-AppVersion
$manifest = Read-Manifest
$faults = @()

if ($manifest.version -ne $version) { $faults += "runtime.json version is '$($manifest.version)' but VERSION is '$version'" }
else { Ok "VERSION matches runtime.json ($version)" }

$bridge = Get-Content (Join-Path $Root "backend\ws_server.py") -Raw -Encoding UTF8
if ($bridge -notmatch '(?m)^SPEAKS\s*=\s*(\d+)') { $faults += "ws_server.py has no SPEAKS" }
elseif ([int]$Matches[1] -ne [int]$manifest.speaks) { $faults += "ws_server.py SPEAKS is $($Matches[1]) but runtime.json speaks is $($manifest.speaks)" }
else { Ok "backend protocol matches runtime.json ($($Matches[1]))" }

$site = Get-Content (Join-Path $Root "docs\index.html") -Raw -Encoding UTF8
$shown = @([regex]::Matches($site, '(?:"softwareVersion":\s*"|<span(?: class="ver")?>)(\d+\.\d+\.\d+)') | ForEach-Object { $_.Groups[1].Value })
$stale = @($shown | Where-Object { $_ -ne $version })
if ($shown.Count -eq 0) { $faults += "docs/index.html shows no version" }
elseif ($stale.Count) { $faults += "docs/index.html shows $($stale[0]) in $($stale.Count) of $($shown.Count) places but VERSION is '$version'" }
else { Ok "docs/index.html shows VERSION in all $($shown.Count) places" }

$faults | ForEach-Object { Fail $_ }
exit [int]($faults.Count -gt 0)
