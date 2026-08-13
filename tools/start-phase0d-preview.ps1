param(
    [int]$Port = 4173,
    [switch]$NoBrowser
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/phase0d-preview"
$npm = "C:/Program Files/nodejs/npm.cmd"
$node = "C:/Program Files/nodejs/node.exe"
if (-not (Test-Path -LiteralPath $npm) -or -not (Test-Path -LiteralPath $node)) {
    throw "Node.js was not found at C:/Program Files/nodejs. Install Node.js 20 or later."
}
if (-not (Test-Path -LiteralPath (Join-Path $webRoot "pkg/engine_host_bg.wasm"))) {
    throw "Generated WASM is missing. Run tools/build-phase0d-wasm.ps1 first."
}

Push-Location $webRoot
try {
    & $npm ci --ignore-scripts
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed with exit code $LASTEXITCODE" }
    & $npm run build
    if ($LASTEXITCODE -ne 0) { throw "preview build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

$url = "http://127.0.0.1:$Port/"
$health = "${url}__health"
try {
    if ((Invoke-RestMethod -Uri $health -TimeoutSec 2).ok) {
        Write-Output "Phase 0D preview is already running."
        Write-Output "url=$url"
        if (-not $NoBrowser) { Start-Process $url }
        exit 0
    }
} catch {}

$stdout = Join-Path $env:TEMP "phase0d-preview-$Port.stdout.log"
$stderr = Join-Path $env:TEMP "phase0d-preview-$Port.stderr.log"
$process = Start-Process -FilePath $node -ArgumentList "server.mjs","--port=$Port" -WorkingDirectory $webRoot -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
$deadline = (Get-Date).AddSeconds(10)
do {
    Start-Sleep -Milliseconds 200
    try { $ready = (Invoke-RestMethod -Uri $health -TimeoutSec 2).ok } catch { $ready = $false }
} while (-not $ready -and (Get-Date) -lt $deadline)
if (-not $ready) {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id }
    throw "Preview server did not become ready. See $stderr"
}
Write-Output "url=$url"
Write-Output "server_pid=$($process.Id)"
Write-Output "runtime=Dedicated Worker WASM core + main-thread WebGPU renderer"
if (-not $NoBrowser) { Start-Process $url }