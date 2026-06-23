$ErrorActionPreference = "Stop"

$classes = "HKCU:\Software\Classes"
$extKey = Join-Path $classes ".dbpx"
$typeKey = Join-Path $classes "DBPX.Image"

if (Test-Path $extKey) {
    Remove-Item -Path $extKey -Recurse -Force
}

if (Test-Path $typeKey) {
    Remove-Item -Path $typeKey -Recurse -Force
}

Write-Host "unregistered .dbpx"
