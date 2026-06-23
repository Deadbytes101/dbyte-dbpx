param(
    [string]$ExePath = ""
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")

if ($ExePath -eq "") {
    Push-Location $root
    try {
        cargo build --release
        $ExePath = Join-Path $root "target\release\dbpx.exe"
    } finally {
        Pop-Location
    }
}

$ExePath = (Resolve-Path $ExePath).Path
$classes = "HKCU:\Software\Classes"
$extKey = Join-Path $classes ".dbpx"
$typeKey = Join-Path $classes "DBPX.Image"
$cmdKey = Join-Path $typeKey "shell\open\command"
$iconKey = Join-Path $typeKey "DefaultIcon"

New-Item -Path $extKey -Force | Out-Null
Set-Item -Path $extKey -Value "DBPX.Image"
Set-ItemProperty -Path $extKey -Name "Content Type" -Value "image/x-dbpx"
Set-ItemProperty -Path $extKey -Name "PerceivedType" -Value "image"

New-Item -Path $typeKey -Force | Out-Null
Set-Item -Path $typeKey -Value "DBPX Image"

New-Item -Path $cmdKey -Force | Out-Null
New-Item -Path $iconKey -Force | Out-Null
Set-Item -Path $iconKey -Value "$ExePath,0"
Set-Item -Path $cmdKey -Value "\"$ExePath\" view \"%1\""

Write-Host "registered .dbpx -> $ExePath view"
