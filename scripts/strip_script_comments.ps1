param(
    [Parameter(Mandatory = $true)][string]$Root,
    [switch]$Check
)

# Takes every comment out of the .ps1 and .bat files under -Root, or with -Check
# fails when any are left. The release runs it on a staged copy, never the tree.
$ErrorActionPreference = "Stop"
$BatchComment = '^\s*(?i:rem(?:\s|$)|::)'

function Get-PsComment([string]$Text, [string]$Path) {
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseInput($Text, [ref]$tokens, [ref]$errors)
    if ($errors.Count -gt 0) { throw "PowerShell parse failed before comment stripping: $Path" }
    return @($tokens |
            Where-Object { $_.Kind -eq [System.Management.Automation.Language.TokenKind]::Comment } |
            Sort-Object { $_.Extent.StartOffset } -Descending)
}

function Join-Line([string[]]$Lines) {
    return ($Lines -join "`r`n").TrimEnd() + "`r`n"
}

$counts = @{ Seen = 0; Changed = 0 }
# Filtered after, as Windows PowerShell ignores -Include alongside -LiteralPath.
$files = Get-ChildItem -LiteralPath $Root -Recurse -File | Where-Object { $_.Extension -in ".ps1", ".bat" }
foreach ($file in $files) {
    $text = [System.IO.File]::ReadAllText($file.FullName)
    $counts.Seen++
    if ($file.Extension -eq ".ps1") {
        $found = Get-PsComment $text $file.FullName
        if ($found.Count -eq 0) { continue }
        if ($Check) { throw "PowerShell comments remain: $($file.FullName)" }
        foreach ($token in $found) {
            $text = $text.Remove($token.Extent.StartOffset, $token.Extent.EndOffset - $token.Extent.StartOffset)
        }
        $text = Join-Line @($text -split "\r?\n" | ForEach-Object { $_.TrimEnd() })
        $encoding = New-Object System.Text.UTF8Encoding($true)
    }
    else {
        $lines = @($text -split "\r?\n")
        $kept = @($lines | Where-Object { $_ -notmatch $BatchComment })
        if ($kept.Count -eq $lines.Count) { continue }
        if ($Check) { throw "Batch comments remain: $($file.FullName)" }
        $text = Join-Line $kept
        $encoding = New-Object System.Text.UTF8Encoding($false)
    }
    [System.IO.File]::WriteAllText($file.FullName, $text, $encoding)
    $counts.Changed++
}

if ($Check) {
    Write-Host "checked $($counts.Seen) PowerShell and batch files and found no comments"
}
else {
    Write-Host "stripped comments from $($counts.Changed) of $($counts.Seen) PowerShell and batch files"
}
