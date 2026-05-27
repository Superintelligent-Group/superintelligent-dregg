param(
    [switch] $TestHarness
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")

Push-Location $repoRoot
try {
    if ($TestHarness) {
        Write-Host "==> cargo test -p dregg-preflight"
        & cargo test -p dregg-preflight
        exit $LASTEXITCODE
    }
    else {
        Write-Host "==> cargo run -p dregg-preflight"
        & cargo run -p dregg-preflight
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}
