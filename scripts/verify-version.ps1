. (Join-Path $PSScriptRoot "common.ps1")

# VERSION, runtime.json and the backend have to agree on the app version and
# the bridge protocol.
$bad = 0
$v = Get-LocalVersion
$mf = Get-RuntimeManifest

if ($mf.app.version -ne $v) { Fail "runtime.json app.version is '$($mf.app.version)' but VERSION is '$v'"; $bad = 1 }
else { Ok "VERSION matches runtime.json ($v)" }

$ws = Get-Content (Join-Path $Root "backend\ws_server.py") -Raw -Encoding UTF8
if ($ws -match 'PROTOCOL_VERSION\s*=\s*(\d+)') {
    if ([int]$Matches[1] -ne [int]$mf.protocol.version) { Fail "ws_server.py PROTOCOL_VERSION is $($Matches[1]) but runtime.json protocol is $($mf.protocol.version)"; $bad = 1 }
    else { Ok "backend protocol matches runtime.json ($($Matches[1]))" }
}
else { Fail "ws_server.py has no PROTOCOL_VERSION"; $bad = 1 }

# Nothing builds the site, so its version is typed in by hand and goes stale
# unless this catches it.
$site = Get-Content (Join-Path $Root "docs\index.html") -Raw -Encoding UTF8
$shown = @([regex]::Matches($site, '(?:"softwareVersion":\s*"|<span(?: class="ver")?>)(\d+\.\d+\.\d+)') | ForEach-Object { $_.Groups[1].Value })
$stale = @($shown | Where-Object { $_ -ne $v })
if ($shown.Count -eq 0) { Fail "docs/index.html shows no version"; $bad = 1 }
elseif ($stale.Count) { Fail "docs/index.html shows $($stale[0]) in $($stale.Count) of $($shown.Count) places but VERSION is '$v'"; $bad = 1 }
else { Ok "docs/index.html shows VERSION in all $($shown.Count) places" }

exit $bad
