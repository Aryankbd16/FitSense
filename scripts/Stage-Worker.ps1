param(
    [string] $ArchivePath,
    [switch] $Download
)

$ErrorActionPreference = 'Stop'
$fitRoot = Split-Path -Parent $PSScriptRoot
$fitVersion = '3.13.17'
$fitName = "python-$fitVersion-embed-amd64.zip"
$fitSha256 = '9E35319BFFEDAB8B36AD44D8F377CB4F1733BCEC257A0B6D04D94F01486DDB38'
if (-not $ArchivePath) { $ArchivePath = Join-Path $fitRoot ".build/downloads/$fitName" }
if (-not (Test-Path -LiteralPath $ArchivePath -PathType Leaf)) {
    if (-not $Download) { throw "Missing $fitName. Supply -ArchivePath or run with -Download." }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent ([IO.Path]::GetFullPath($ArchivePath))) | Out-Null
    Invoke-WebRequest -Uri "https://www.python.org/ftp/python/$fitVersion/$fitName" -OutFile $ArchivePath
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash -ne $fitSha256) {
    throw 'The CPython archive does not match the pinned SHA-256.'
}

$fitBuild = Join-Path $fitRoot '.build'
$fitStaging = Join-Path $fitBuild ("python-staging-" + [guid]::NewGuid().ToString('N'))
$fitRuntime = Join-Path $fitRoot 'src-tauri/runtime'
$fitWorker = Join-Path $fitRuntime 'worker'
Expand-Archive -LiteralPath $ArchivePath -DestinationPath $fitStaging
Copy-Item -LiteralPath (Join-Path $fitRoot 'python-worker/worker.py') -Destination $fitStaging
$fitReported = & (Join-Path $fitStaging 'python.exe') -I -B --version
if ($LASTEXITCODE -ne 0 -or $fitReported -ne "Python $fitVersion") { throw 'The embedded interpreter failed verification.' }
New-Item -ItemType Directory -Force -Path $fitRuntime | Out-Null
if (Test-Path -LiteralPath $fitWorker) {
    $fitOld = Get-Item -LiteralPath $fitWorker -Force
    if ($fitOld.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'The worker staging path is a reparse point.' }
    $fitPrevious = Join-Path $fitBuild ("python-previous-" + [guid]::NewGuid().ToString('N'))
    Move-Item -LiteralPath $fitWorker -Destination $fitPrevious
}
try { Move-Item -LiteralPath $fitStaging -Destination $fitWorker }
catch {
    if ($fitPrevious) { Move-Item -LiteralPath $fitPrevious -Destination $fitWorker }
    throw
}
Write-Output "Staged $fitReported at $fitWorker"
