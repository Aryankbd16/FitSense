$ErrorActionPreference = 'Stop'
$fitRoot = Split-Path -Parent $PSScriptRoot
$fitBinary = Join-Path $fitRoot 'src-tauri/target/x86_64-pc-windows-msvc/release/fitsense.exe'
$fitWorker = Join-Path $fitRoot 'src-tauri/runtime/worker'
$fitLicenses = Join-Path $fitRoot 'src-tauri/runtime/licenses'
if (-not (Test-Path -LiteralPath $fitBinary -PathType Leaf)) { throw 'Build the Windows x64 release first.' }
if (-not (Test-Path -LiteralPath (Join-Path $fitWorker 'python.exe') -PathType Leaf)) { throw 'Stage the embeddable Python worker first.' }
if (-not (Test-Path -LiteralPath $fitLicenses -PathType Container)) { throw 'Build the frontend to stage its third-party notices.' }
$fitVersion = (Get-Content -Raw -LiteralPath (Join-Path $fitRoot 'src-tauri/tauri.conf.json') | ConvertFrom-Json).version
$fitStaging = Join-Path $fitRoot ('.build/portable-staging-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $fitStaging 'runtime') | Out-Null
Copy-Item -LiteralPath $fitBinary -Destination (Join-Path $fitStaging 'FitSense.exe')
Copy-Item -LiteralPath $fitWorker -Destination (Join-Path $fitStaging 'runtime') -Recurse
Copy-Item -LiteralPath $fitLicenses -Destination (Join-Path $fitStaging 'runtime') -Recurse
New-Item -ItemType File -Path (Join-Path $fitStaging 'FitSense.portable') | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'PORTABLE_README.txt') -Destination (Join-Path $fitStaging 'README.txt')
$fitOut = Join-Path $fitRoot 'artifacts'
New-Item -ItemType Directory -Force -Path $fitOut | Out-Null
$fitZip = Join-Path $fitOut "FitSense-$fitVersion-windows-x64-portable.zip"
Compress-Archive -Path (Join-Path $fitStaging '*') -DestinationPath $fitZip -Force
Write-Output $fitZip
