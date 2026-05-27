param(
    [switch] $CargoTest,
    [switch] $IncludeIgnored
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")

function Test-NextestAvailable {
    & cargo nextest --version *> $null
    return $LASTEXITCODE -eq 0
}

Push-Location $repoRoot
try {
    if (-not $CargoTest -and -not $IncludeIgnored -and (Test-NextestAvailable)) {
        Write-Host "==> cargo nextest run --profile full"
        & cargo nextest run --profile full
        exit $LASTEXITCODE
    }

    $args = @("test", "--workspace")
    if ($IncludeIgnored) {
        $args += @("--", "--include-ignored")
    }

    Write-Host "==> cargo $($args -join ' ')"
    & cargo @args
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
