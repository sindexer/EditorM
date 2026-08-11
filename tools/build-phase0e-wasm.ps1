param(
    [string]$OutDir = "web/editor/public/pkg"
)

$ErrorActionPreference = "Stop"
$phase0dBuilder = Join-Path $PSScriptRoot "build-phase0d-wasm.ps1"
& powershell -NoProfile -ExecutionPolicy Bypass -File $phase0dBuilder -OutDir $OutDir
if ($LASTEXITCODE -ne 0) { throw "Phase 0E WASM build failed with exit code $LASTEXITCODE" }
Write-Output "phase0e_wasm=$OutDir"
