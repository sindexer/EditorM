param(
    [string]$OutDir = "web/phase0d-preview/pkg"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$toolRoot = if ($env:VAE_TOOL_ROOT) { $env:VAE_TOOL_ROOT } else { Join-Path $repoRoot ".tools" }
$wasmBindgen = if ($env:VAE_WASM_BINDGEN) {
    $env:VAE_WASM_BINDGEN
} elseif (Get-Command wasm-bindgen -ErrorAction SilentlyContinue) {
    (Get-Command wasm-bindgen).Source
} else {
    Get-ChildItem -LiteralPath (Join-Path $toolRoot "wasm-bindgen-0.2.126") -Recurse -Filter wasm-bindgen.exe -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $wasmBindgen -or -not (Test-Path -LiteralPath $wasmBindgen)) {
    throw "wasm-bindgen CLI 0.2.126 was not found. Set VAE_WASM_BINDGEN to the verified executable."
}

Push-Location $repoRoot
try {
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw "wasm32 release build failed with exit code $LASTEXITCODE" }
    $wasm = Join-Path $repoRoot "target/wasm32-unknown-unknown/release/visual_authoring_wasm_bridge.wasm"
    $output = Join-Path $repoRoot $OutDir
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    & $wasmBindgen --target web --out-dir $output --out-name engine_host $wasm
    if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed with exit code $LASTEXITCODE" }
    Write-Output "wasm=$wasm"
    Write-Output "web_module=$output"
} finally {
    Pop-Location
}