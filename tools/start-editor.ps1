param(
    [int]$Port = 0,
    [switch]$NoBrowser
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/editor"
$npm = "C:/Program Files/nodejs/npm.cmd"
$node = "C:/Program Files/nodejs/node.exe"

if ($Port -eq 0) {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $Port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
}
if ($Port -lt 1 -or $Port -gt 65535) { throw "Invalid port $Port" }

Push-Location $repoRoot
$server = $null
try {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "build-phase0e-wasm.ps1")
    if ($LASTEXITCODE -ne 0) { throw "WASM build failed" }
    Push-Location $webRoot
    try {
        if (-not (Test-Path -LiteralPath (Join-Path $webRoot "node_modules"))) {
            & $npm ci --ignore-scripts
            if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
        }
        & $npm run build
        if ($LASTEXITCODE -ne 0) { throw "React production build failed" }
        $server = Start-Process -FilePath $node -ArgumentList @("server.mjs", "--port=$Port") -WorkingDirectory $webRoot -PassThru -WindowStyle Hidden
    } finally {
        Pop-Location
    }

    $url = "http://127.0.0.1:$Port/"
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        try {
            $health = Invoke-RestMethod -Uri ($url + "__health") -TimeoutSec 1
            if ($health.ok) { break }
        } catch {
            Start-Sleep -Milliseconds 100
        }
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $health.ok) { throw "editor_preview_not_ready: health endpoint did not become ready" }

    $assets = @(
        @{ Path = "index.html"; Mime = "text/html" },
        @{ Path = "worker.js"; Mime = "text/javascript" },
        @{ Path = "pkg/engine_host.js"; Mime = "text/javascript" },
        @{ Path = "pkg/engine_host_bg.wasm"; Mime = "application/wasm" }
    )
    foreach ($asset in $assets) {
        $response = Invoke-WebRequest -Uri ($url + $asset.Path) -UseBasicParsing -TimeoutSec 5
        if ($response.StatusCode -ne 200) { throw "phase0e_asset_http_error: $($asset.Path)" }
        if (-not $response.Headers["Content-Type"].StartsWith($asset.Mime)) {
            throw "phase0e_asset_mime_error: $($asset.Path) returned $($response.Headers['Content-Type'])"
        }
    }
    Write-Output "EditorM Phase 1A: $url"
    Write-Output "runtime=Dedicated Worker WASM core + shared analytic-AA actual WebGPU renderer"
    Write-Output "health=PASS assets=PASS mime=PASS; stop=Ctrl+C"
    if (-not $NoBrowser) { Start-Process $url }
    $server.WaitForExit()
} finally {
    if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
    Pop-Location
}
