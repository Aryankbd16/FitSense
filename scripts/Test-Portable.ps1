param([string] $ArchivePath, [switch] $CheckForcedExit)

$ErrorActionPreference = 'Stop'
$fitRoot = Split-Path -Parent $PSScriptRoot
if (-not $ArchivePath) {
    $fitVersion = (Get-Content -Raw -LiteralPath (Join-Path $fitRoot 'src-tauri/tauri.conf.json') | ConvertFrom-Json).version
    $ArchivePath = Join-Path $fitRoot "artifacts/FitSense-$fitVersion-windows-x64-portable.zip"
}
$fitRun = Join-Path $fitRoot ('.build/portable smoke ' + [guid]::NewGuid().ToString('N'))
$fitScreens = Join-Path $fitRun 'screenshots'
Expand-Archive -LiteralPath $ArchivePath -DestinationPath $fitRun
New-Item -ItemType Directory -Path $fitScreens | Out-Null
$fitApp = Join-Path $fitRun 'FitSense.exe'
$fitPort = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$fitPort.Start()
$fitPortNumber = $fitPort.LocalEndpoint.Port
$fitPort.Stop()

# A temporary loopback DevTools endpoint is enabled only in this smoke test.
$fitPreviousArgs = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$fitPortNumber --disable-background-networking --host-resolver-rules=`"MAP * ~NOTFOUND, EXCLUDE localhost, EXCLUDE tauri.localhost, EXCLUDE ipc.localhost`""
try {
    $fitProcess = Start-Process -FilePath $fitApp -WorkingDirectory ([IO.Path]::GetTempPath()) -PassThru
    node (Join-Path $fitRoot 'tests/desktop-smoke.mjs') $fitPortNumber $fitScreens
    if ($LASTEXITCODE -ne 0) {
        $fitProcess.Refresh()
        $fitStatus = if ($fitProcess.HasExited) { "process exit $($fitProcess.ExitCode)" } else { "window title '$($fitProcess.MainWindowTitle)'" }
        throw "The rendered desktop smoke check failed ($fitStatus)."
    }
    $fitProcess.Refresh()
    if (-not $fitProcess.CloseMainWindow() -or -not $fitProcess.WaitForExit(8000) -or $fitProcess.ExitCode -ne 0) {
        throw 'FitSense did not close cleanly.'
    }
    $fitData = Join-Path $fitRun 'data'
    if (-not (Test-Path -LiteralPath (Join-Path $fitData 'db/foundation.sqlite3') -PathType Leaf)) {
        throw 'Portable SQLite database was missing.'
    }
    $fitEvents = Get-ChildItem -LiteralPath (Join-Path $fitData 'logs') -Filter 'session-*.jsonl' |
        Get-Content | ForEach-Object { (ConvertFrom-Json -InputObject $_).event }
    foreach ($fitEvent in @('storage_ready', 'worker_ready', 'worker_diagnostics', 'python_stopped', 'app_stopped')) {
        if ($fitEvent -notin $fitEvents) { throw "Missing expected event $fitEvent" }
    }
    if ('worker_forced_stop' -in $fitEvents) { throw 'Normal closure needed forced worker termination.' }
    if ($CheckForcedExit) {
        $fitProcess = Start-Process -FilePath $fitApp -WorkingDirectory ([IO.Path]::GetTempPath()) -PassThru
        $fitWorker = $null
        $fitPython = Join-Path $fitRun 'runtime/worker/python.exe'
        $fitDeadline = [DateTime]::UtcNow.AddSeconds(12)
        while (-not $fitWorker -and [DateTime]::UtcNow -lt $fitDeadline) {
            $fitWorker = Get-Process -Name python -ErrorAction SilentlyContinue | Where-Object Path -EQ $fitPython | Select-Object -First 1
            Start-Sleep -Milliseconds 100
        }
        if (-not $fitWorker) { throw 'The bundled worker process was not found.' }
        Stop-Process -InputObject $fitProcess
        if (-not $fitWorker.WaitForExit(5000)) { throw 'The worker survived termination of FitSense.' }
        Write-Output 'Forced parent termination stopped the bundled Python process.'
    }
    Write-Output "Verified the extracted portable archive. Screenshots are in $fitScreens"
} finally {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $fitPreviousArgs
    if ($fitProcess -and -not $fitProcess.HasExited) { Stop-Process -InputObject $fitProcess }
}
