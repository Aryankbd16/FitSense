param([string] $ArchivePath, [switch] $Installed)

$ErrorActionPreference = 'Stop'
$fitRoot = Split-Path -Parent $PSScriptRoot
if (-not $ArchivePath) {
    $fitVersion = (Get-Content -Raw (Join-Path $fitRoot 'src-tauri/tauri.conf.json') | ConvertFrom-Json).version
    $ArchivePath = Join-Path $fitRoot "artifacts/FitSense-$fitVersion-windows-x64-portable.zip"
}
$fitRun = Join-Path $fitRoot ('.build/wardrobe smoke ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fitRun | Out-Null
$fitPython = Join-Path $fitRoot 'src-tauri/runtime/worker/python.exe'
$fitInspector = Join-Path $fitRoot 'tests/inspect-vault.py'
$fitImage = Join-Path $fitRun 'fixture.png'
& $fitPython -I -B $fitInspector --fixture $fitImage
if ($LASTEXITCODE) { throw 'Could not create the synthetic test photo.' }

function Expand-TestApp([string] $Label) {
    $fitDestination = Join-Path $fitRun $Label
    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $fitDestination
    New-Item -ItemType Directory -Path (Join-Path $fitDestination 'screenshots') | Out-Null
    return $fitDestination
}

function Invoke-WardrobeWorkflow([string] $Directory, [string] $Workflow, [string] $Mode, [string] $Fixture) {
    $fitSocket = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $fitSocket.Start()
    $fitPort = $fitSocket.LocalEndpoint.Port
    $fitSocket.Stop()
    $fitPrevious = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = ('--remote-debugging-port={0} --disable-background-networking --host-resolver-rules="MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE tauri.localhost, EXCLUDE ipc.localhost"' -f $fitPort)
    $fitProcess = $null
    try {
        $fitProcess = Start-Process -FilePath (Join-Path $Directory 'FitSense.exe') -WorkingDirectory $fitRun -PassThru
        node (Join-Path $fitRoot 'tests/wardrobe-smoke.mjs') $fitPort (Join-Path $Directory 'screenshots') $Workflow $Mode $Fixture
        if ($LASTEXITCODE) { throw "The $Workflow workflow failed." }
        $fitProcess.Refresh()
        if (-not $fitProcess.CloseMainWindow() -or -not $fitProcess.WaitForExit(8000) -or $fitProcess.ExitCode -ne 0) {
            throw 'FitSense did not close cleanly.'
        }
    } finally {
        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $fitPrevious
        if ($fitProcess -and -not $fitProcess.HasExited) { Stop-Process -InputObject $fitProcess }
    }
}

$fitSource = Expand-TestApp 'source'
$fitMode = 'portable'
$fitSourceData = Join-Path $fitSource 'data'
$fitInstalledRoot = $null
if ($Installed) {
    $fitMode = 'installed'
    $fitKnownFolder = [Environment]::GetFolderPath('LocalApplicationData')
    if (-not $fitKnownFolder) { throw 'The current account has no known LocalApplicationData folder.' }
    $fitSourceData = Join-Path $fitKnownFolder 'com.fitsense.desktop'
    if (Test-Path -LiteralPath $fitSourceData) { throw 'Installed-mode testing requires an unused FitSense workspace for this account.' }
    Remove-Item -LiteralPath (Join-Path $fitSource 'FitSense.portable')
    $fitInstalledRoot = $fitSourceData
}

try {
    Invoke-WardrobeWorkflow $fitSource 'source' $fitMode $fitImage
    $fitPackage = Get-ChildItem -LiteralPath (Join-Path $fitSourceData 'backups') -Filter '*.fitbackup' -File |
        Select-Object -First 1
    if (-not $fitPackage) { throw 'The desktop workflow did not create a backup.' }
    foreach ($fitVariant in @('originals', 'display', 'thumbnails')) {
        $fitDeleted = Get-ChildItem -LiteralPath (Join-Path $fitSourceData "private/vault/images/$fitVariant") -File
        if ($fitDeleted) { throw 'An encrypted asset survived successful deletion.' }
    }

    $fitRestored = Expand-TestApp 'restored'
    $fitBackupDirectory = Join-Path $fitRestored 'data/backups'
    New-Item -ItemType Directory -Path $fitBackupDirectory -Force | Out-Null
    Copy-Item -LiteralPath $fitPackage.FullName -Destination $fitBackupDirectory
    Invoke-WardrobeWorkflow $fitRestored 'restore' 'portable' $fitPackage.Name
    $fitRestoredVault = Join-Path $fitRestored 'data/private/vault'
    & $fitPython -I -B $fitInspector --verify $fitRestoredVault
    if ($LASTEXITCODE) { throw 'The vault did not pass the independent SQLite and image checks.' }

    $fitMigrated = Expand-TestApp 'portable recovery'
    $fitMigratedVault = Join-Path $fitMigrated 'data/private/vault'
    New-Item -ItemType Directory -Path $fitMigratedVault -Force | Out-Null
    foreach ($fitItem in @('wardrobe.sqlite3', 'recovery.key', 'images')) {
        Copy-Item -LiteralPath (Join-Path $fitRestoredVault $fitItem) -Destination $fitMigratedVault -Recurse
    }
    Invoke-WardrobeWorkflow $fitMigrated 'recover' 'portable' ''
    if (-not (Test-Path -LiteralPath (Join-Path $fitMigratedVault 'local.key') -PathType Leaf)) {
        throw 'Recovery did not create a local Windows key wrapper.'
    }
    Write-Output "Verification artifacts are in $fitRun"
} finally {
    if ($fitInstalledRoot -and (Test-Path -LiteralPath $fitInstalledRoot)) {
        $fitActual = Get-Item -LiteralPath $fitInstalledRoot -Force
        if ($fitActual.FullName -ne $fitInstalledRoot -or ($fitActual.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Installed test workspace changed location and was left in place.'
        }
        Move-Item -LiteralPath $fitInstalledRoot -Destination (Join-Path $fitRun 'installed fixture data')
    }
}
