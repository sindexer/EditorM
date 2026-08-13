param(
    [int]$Port = 4175
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/phase0d-preview"
$npm = "C:/Program Files/nodejs/npm.cmd"
$node = "C:/Program Files/nodejs/node.exe"
$server = $null
Push-Location $repoRoot
try {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0d_proof -- docs/PHASE_0D_METRICS.json
    if ($LASTEXITCODE -ne 0) { throw "Native wgpu proof failed with exit code $LASTEXITCODE" }
    $native = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "docs/PHASE_0D_METRICS.json") | ConvertFrom-Json
    if (-not $native.all_passed) { throw "Native proof JSON reports failure" }

    Push-Location $webRoot
    try {
        & $npm ci --ignore-scripts
        if ($LASTEXITCODE -ne 0) { throw "npm ci failed with exit code $LASTEXITCODE" }
        & $npm run build
        if ($LASTEXITCODE -ne 0) { throw "Web build failed with exit code $LASTEXITCODE" }
        $stdout = Join-Path $env:TEMP "phase0d-proof-$Port.stdout.log"
        $stderr = Join-Path $env:TEMP "phase0d-proof-$Port.stderr.log"
        $server = Start-Process -FilePath $node -ArgumentList "server.mjs","--port=$Port" -WorkingDirectory $webRoot -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
        $health = "http://127.0.0.1:$Port/__health"
        $deadline = (Get-Date).AddSeconds(10)
        do {
            Start-Sleep -Milliseconds 200
            try { $ready = (Invoke-RestMethod -Uri $health -TimeoutSec 2).ok } catch { $ready = $false }
        } while (-not $ready -and (Get-Date) -lt $deadline)
        if (-not $ready) { throw "Proof server did not become ready. See $stderr" }
        $env:PHASE0D_URL = "http://127.0.0.1:$Port/"
        & $npm run test:browser
        if ($LASTEXITCODE -ne 0) { throw "Actual browser proof failed with exit code $LASTEXITCODE" }
    } finally {
        Remove-Item Env:PHASE0D_URL -ErrorAction SilentlyContinue
        Pop-Location
    }
    $browser = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "docs/verification/PHASE_0D_R1_BROWSER_PROOF.json") | ConvertFrom-Json
    if (-not $browser.all_passed) { throw "Browser proof JSON reports failure" }
    Write-Output "native_metrics=docs/PHASE_0D_METRICS.json"
    Write-Output "browser_proof=docs/verification/PHASE_0D_R1_BROWSER_PROOF.json"
    Write-Output "screenshot=docs/verification/phase0d-r1-preview.png"
    Write-Output "all_passed=true"
} finally {
    if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id }
    Pop-Location
}