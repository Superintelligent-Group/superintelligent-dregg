param(
    [switch] $Json
)

$ErrorActionPreference = "Stop"

$metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json

$packages = $metadata.packages | Sort-Object name | ForEach-Object {
    [pscustomobject]@{
        Name = $_.name
        Manifest = (Resolve-Path -LiteralPath $_.manifest_path -Relative)
        Description = $_.description
        License = $_.license
        Readme = $_.readme
        MissingDescription = [string]::IsNullOrWhiteSpace($_.description)
        MissingLicense = [string]::IsNullOrWhiteSpace($_.license)
        MissingReadme = [string]::IsNullOrWhiteSpace($_.readme)
    }
}

$summary = [pscustomobject]@{
    Packages = @($packages).Count
    MissingDescription = @($packages | Where-Object MissingDescription).Count
    MissingLicense = @($packages | Where-Object MissingLicense).Count
    MissingReadme = @($packages | Where-Object MissingReadme).Count
}

if ($Json) {
    [pscustomobject]@{
        Summary = $summary
        Packages = $packages
    } | ConvertTo-Json -Depth 4
    exit 0
}

Write-Host "Workspace package metadata"
Write-Host "Packages:            $($summary.Packages)"
Write-Host "Missing description: $($summary.MissingDescription)"
Write-Host "Missing license:     $($summary.MissingLicense)"
Write-Host "Missing readme:      $($summary.MissingReadme)"
Write-Host ""

$packages |
    Where-Object { $_.MissingDescription -or $_.MissingLicense -or $_.MissingReadme } |
    Select-Object Name, MissingDescription, MissingLicense, MissingReadme, Manifest |
    Format-Table -AutoSize
