param(
    [string[]] $Package = @("dregg-cli"),
    [switch] $AllTargets,
    [switch] $SkipDocs,
    [switch] $SkipMetadata,
    [switch] $SkipFmt
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")

function Invoke-Step {
    param(
        [string] $Name,
        [scriptblock] $Command
    )

    Write-Host ""
    Write-Host "==> $Name"
    & $Command
}

function Invoke-External {
    param([string[]] $CommandLine)

    $command = $CommandLine[0]
    $commandArgs = @()
    if ($CommandLine.Count -gt 1) {
        $commandArgs = $CommandLine[1..($CommandLine.Count - 1)]
    }

    & $command @commandArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Push-Location $repoRoot
try {
    if (-not $SkipDocs) {
        Invoke-Step "Markdown link check" {
            & (Join-Path $PSScriptRoot "docs-check.ps1")
        }
    }

    if (-not $SkipMetadata) {
        Invoke-Step "Workspace package metadata" {
            & (Join-Path $PSScriptRoot "workspace-package-report.ps1")
        }
    }

    if (-not $SkipFmt) {
        Invoke-Step "Rust formatting" {
            Invoke-External @("cargo", "fmt", "--all", "--", "--check")
        }
    }

    foreach ($pkg in $Package) {
        Invoke-Step "cargo check -p $pkg" {
            $args = @("check", "-p", $pkg)
            if ($AllTargets) {
                $args += "--all-targets"
            }
            Invoke-External (@("cargo") + $args)
        }
    }
}
finally {
    Pop-Location
}
