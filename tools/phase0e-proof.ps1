param(
    [string]$MetricsPath = "docs/PHASE_0E_METRICS.json"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/editor"
$npm = "C:/Program Files/nodejs/npm.cmd"

Push-Location $repoRoot
try {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0e_proof -- $MetricsPath
    if ($LASTEXITCODE -ne 0) { throw "Phase 0E native proof failed with exit code $LASTEXITCODE" }

    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "build-phase0e-wasm.ps1")
    if ($LASTEXITCODE -ne 0) { throw "Phase 0E WASM build failed with exit code $LASTEXITCODE" }

    Push-Location $webRoot
    try {
        & $npm run test:browser
        if ($LASTEXITCODE -ne 0) { throw "Phase 0E actual hardware browser proof failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }

    $native = Get-Content -Raw -LiteralPath (Join-Path $repoRoot $MetricsPath) | ConvertFrom-Json
    $browserPath = Join-Path $repoRoot "docs/verification/PHASE_0E_BROWSER_PROOF.json"
    $pixelPath = Join-Path $repoRoot "docs/verification/PHASE_0E_PIXEL_READBACK.json"
    $browser = Get-Content -Raw -LiteralPath $browserPath | ConvertFrom-Json
    $pixel = Get-Content -Raw -LiteralPath $pixelPath | ConvertFrom-Json
    if (-not $native.all_passed) { throw "Phase 0E native metrics report failure" }
    if (-not $browser.all_passed) { throw "Phase 0E browser proof reports failure" }
    if (-not $browser.harness.self_contained_server) { throw "Browser proof did not manage its own server" }
    if (-not $browser.hardware.actual_hardware) { throw "Browser proof did not use actual hardware" }
    if (-not $pixel.all_passed) { throw "Phase 0E pixel readback reports failure" }
    if (@($browser.screenshots.PSObject.Properties).Count -ne 5) { throw "Expected five Phase 0E screenshots" }

    Write-Output "native_metrics=$MetricsPath"
    Write-Output "browser_proof=docs/verification/PHASE_0E_BROWSER_PROOF.json"
    Write-Output "pixel_readback=docs/verification/PHASE_0E_PIXEL_READBACK.json"
    Write-Output "screenshots=5"
    Write-Output "all_passed=true"
} finally {
    Pop-Location
}
