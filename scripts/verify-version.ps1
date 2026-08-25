. (Join-Path $PSScriptRoot "common.ps1")




$bad = 0
$v = Get-LocalVersion
$mf = Get-RuntimeManifest

if ($mf.app.version -ne $v) { Fail "runtime.json app.version '$($mf.app.version)' != VERSION '$v'"; $bad = 1 }
else { Ok "VERSION == runtime.json ($v)" }

$ws = Get-Content (Join-Path $Root "backend\ws_server.py") -Raw -Encoding UTF8
if ($ws -match 'PROTOCOL_VERSION\s*=\s*(\d+)') {
    if ([int]$Matches[1] -ne [int]$mf.protocol.version) { Fail "ws_server.py PROTOCOL_VERSION $($Matches[1]) != runtime.json protocol $($mf.protocol.version)"; $bad = 1 }
    else { Ok "backend protocol == runtime.json ($($Matches[1]))" }
}
else { Fail "ws_server.py has no PROTOCOL_VERSION"; $bad = 1 }

$tuiBridge = Join-Path $Root "tui\src\bridge.ts"
if (Test-Path $tuiBridge) {
    $t = Get-Content $tuiBridge -Raw -Encoding UTF8
    if ($t -match 'const PROTOCOL\s*=\s*(\d+)') {
        if ([int]$Matches[1] -ne [int]$mf.protocol.version) { Fail "tui bridge.ts PROTOCOL $($Matches[1]) != runtime.json protocol $($mf.protocol.version)"; $bad = 1 }
        else { Ok "tui protocol == runtime.json ($($Matches[1]))" }
    }
    else { Fail "tui/src/bridge.ts has no PROTOCOL constant"; $bad = 1 }

    $pkg = Get-Content (Join-Path $Root "tui\package.json") -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($pkg.version -ne $v) { Fail "tui package.json version '$($pkg.version)' != VERSION '$v'"; $bad = 1 }
    else { Ok "tui package.json == VERSION ($v)" }

    # A stale bundle is worse than no bundle: it silently runs last week's UI.
    $bundle = Join-Path $Root "tui\dist\overseer.js"
    if (-not (Test-Path $bundle)) { Fail "tui/dist/overseer.js is missing - run: cd tui; npm run build"; $bad = 1 }
    else {
        $bundleAt = (Get-Item $bundle).LastWriteTimeUtc
        $stale = @(Get-ChildItem (Join-Path $Root "tui\src") -Recurse -File |
            Where-Object { $_.Extension -in ".ts", ".tsx" -and $_.LastWriteTimeUtc -gt $bundleAt })
        if ($stale.Count -gt 0) {
            Fail "tui/dist/overseer.js is older than $($stale.Count) source file(s) - run: cd tui; npm run build"; $bad = 1
        }
        else { Ok "tui bundle is newer than every source file" }
    }
}
else {
    Ok "slim tree (no tui sources) - tui checks skipped"
}

exit $bad
