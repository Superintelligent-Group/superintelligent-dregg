param(
    [string[]] $Package = @("dregg-types"),
    [switch] $CargoTest,
    [switch] $Lib
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")

function Test-NextestAvailable {
    & cargo nextest --version *> $null
    return $LASTEXITCODE -eq 0
}

function Get-PackageArgs {
    $args = @()
    foreach ($pkg in $Package) {
        $args += @("-p", $pkg)
    }
    return $args
}

Push-Location $repoRoot
try {
    $packageArgs = Get-PackageArgs

    if (-not $CargoTest -and (Test-NextestAvailable)) {
        Write-Host "==> cargo nextest run $($packageArgs -join ' ')"
        & cargo nextest run @packageArgs
        exit $LASTEXITCODE
    }

    $args = @("test") + $packageArgs
    if ($Lib) {
        $args += "--lib"
    }

    Write-Host "==> cargo $($args -join ' ')"
    & cargo @args
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
