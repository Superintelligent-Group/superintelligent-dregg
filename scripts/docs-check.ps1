param(
    [string[]] $Paths = @("README.md", "HANDOFF.md", "TOPLEVEL-MD-INDEX.md", "apps/README.md", "docs")
)

$ErrorActionPreference = "Stop"

function Resolve-InputPath {
    param([string] $Path)

    if (Test-Path -LiteralPath $Path -PathType Container) {
        Get-ChildItem -LiteralPath $Path -Recurse -Filter *.md -File
        return
    }

    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        Get-Item -LiteralPath $Path
        return
    }

    throw "Path does not exist: $Path"
}

$files = foreach ($path in $Paths) {
    Resolve-InputPath -Path $path
}

$files = $files | Sort-Object FullName -Unique
$missing = New-Object System.Collections.Generic.List[object]

foreach ($file in $files) {
    $text = Get-Content -LiteralPath $file.FullName -Raw
    $matches = [regex]::Matches($text, '\[[^\]]+\]\(([^)]+)\)')

    foreach ($match in $matches) {
        $link = $match.Groups[1].Value.Trim()

        if ($link -match '^(https?:|mailto:|#)') {
            continue
        }

        $target = $link.Split('#')[0]
        if ([string]::IsNullOrWhiteSpace($target)) {
            continue
        }

        $resolved = Join-Path $file.DirectoryName $target
        if (-not (Test-Path -LiteralPath $resolved)) {
            $missing.Add([pscustomobject]@{
                File = Resolve-Path -LiteralPath $file.FullName -Relative
                Link = $link
                Resolved = $resolved
            })
        }
    }
}

if ($missing.Count -gt 0) {
    $missing | Format-Table -AutoSize
    Write-Error "Missing local markdown links: $($missing.Count)"
    exit 1
}

Write-Host "All local markdown links resolve across $($files.Count) markdown files."
