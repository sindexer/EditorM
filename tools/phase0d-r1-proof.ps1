param(
    [string]$MetricsPath = "docs/PHASE_0D_R1_METRICS.json"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/phase0d-preview"
$npm = "C:/Program Files/nodejs/npm.cmd"

Push-Location $repoRoot
try {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0d_r1_proof -- $MetricsPath
    if ($LASTEXITCODE -ne 0) { throw "Native R1 proof failed with exit code $LASTEXITCODE" }

    Push-Location $webRoot
    try {
        Remove-Item Env:PHASE0D_URL -ErrorAction SilentlyContinue
        & $npm run test:browser
        if ($LASTEXITCODE -ne 0) { throw "Self-contained actual browser proof failed with exit code $LASTEXITCODE" }
    } finally {
        Remove-Item Env:PHASE0D_URL -ErrorAction SilentlyContinue
        Pop-Location
    }

    $native = Get-Content -Raw -LiteralPath (Join-Path $repoRoot $MetricsPath) | ConvertFrom-Json
    $browserPath = Join-Path $repoRoot "docs/verification/PHASE_0D_R1_BROWSER_PROOF.json"
    $pixelPath = Join-Path $repoRoot "docs/verification/PHASE_0D_R1_PIXEL_READBACK.json"
    $browser = Get-Content -Raw -LiteralPath $browserPath | ConvertFrom-Json
    $pixel = Get-Content -Raw -LiteralPath $pixelPath | ConvertFrom-Json
    if (-not $native.all_passed) { throw "Native R1 metrics report failure" }
    if (-not $browser.all_passed) { throw "Browser R1 proof reports failure" }
    if (-not $browser.harness.self_contained_server) { throw "Browser proof did not manage its own server" }
    if (-not $browser.hardware.actual_hardware) { throw "Browser proof did not use actual hardware" }
    if (-not $pixel.all_passed) { throw "Pixel readback proof reports failure" }

    Write-Output "native_metrics=$MetricsPath"
    Write-Output "browser_proof=docs/verification/PHASE_0D_R1_BROWSER_PROOF.json"
    Write-Output "pixel_readback=docs/verification/PHASE_0D_R1_PIXEL_READBACK.json"
    Write-Output "screenshot=docs/verification/phase0d-r1-preview.png"
    Write-Output "all_passed=true"
} finally {
    Pop-Location
}
