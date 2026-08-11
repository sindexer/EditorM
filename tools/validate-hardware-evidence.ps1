[CmdletBinding()]
param(
    [string]$RepositoryRoot,
    [Parameter(Mandatory = $true)]
    [string]$EvidencePath,
    [Parameter(Mandatory = $true)]
    [string]$EvidenceSha256,
    [int]$MaximumAgeHours = 72
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}

$repoRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$relative = $EvidencePath.Replace("\", "/").Trim()
if ([string]::IsNullOrWhiteSpace($relative) -or [IO.Path]::IsPathRooted($relative)) {
    throw "Hardware evidence path must be repository-relative"
}
$segments = $relative.Split("/", [StringSplitOptions]::RemoveEmptyEntries)
if ($segments -contains ".." -or $relative -notmatch "^docs/verification/.+\.json$") {
    throw "Hardware evidence must be a JSON file below docs/verification"
}
if ($EvidenceSha256 -notmatch "^[0-9a-fA-F]{64}$") { throw "Hardware evidence SHA-256 is invalid" }

$verificationRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "docs/verification")) + [IO.Path]::DirectorySeparatorChar
$fullPath = [IO.Path]::GetFullPath((Join-Path $repoRoot $relative))
if (-not $fullPath.StartsWith($verificationRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Hardware evidence path escaped docs/verification"
}
if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { throw "Hardware evidence file does not exist: $relative" }
$null = & git -C $repoRoot ls-files --error-unmatch -- $relative 2>$null
if ($LASTEXITCODE -ne 0) { throw "Hardware evidence must be tracked by Git: $relative" }

$actualHash = (Get-FileHash -LiteralPath $fullPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $EvidenceSha256.ToLowerInvariant()) { throw "Hardware evidence SHA-256 does not match the tracked file" }

try {
    $proof = Get-Content -Raw -LiteralPath $fullPath | ConvertFrom-Json
}
catch {
    throw "Hardware evidence is not valid JSON"
}

if ($proof.proof_kind -ne "actual-hardware-browser") { throw "Evidence is not an actual-hardware browser proof" }
if ($proof.hardware.actual_hardware -ne $true -or $proof.runtime.actual_webgpu -ne $true) { throw "Evidence does not prove actual WebGPU hardware use" }
if ($proof.runtime.backend -notmatch "browser-webgpu") { throw "Evidence backend is not browser WebGPU" }
if ($proof.browser.mode -notmatch "no mock" -or $proof.browser.mode -notmatch "no Canvas2D fallback") { throw "Evidence does not explicitly exclude mock and Canvas2D fallback" }
if ($proof.max_fallback_rebuild_count_seen -ne 0) { throw "Evidence reports a fallback or rebuild" }
if ($proof.all_passed -ne $true -or $proof.checks.actual_hardware_webgpu -ne $true) { throw "Hardware proof did not pass all checks" }
if ($proof.checks.actual_pixel_readback -ne $true) { throw "Hardware proof does not contain a passing pixel readback" }
if ($proof.checks.create_shape_dom_pointer -ne $true -or $proof.checks.undo_redo_dom_controls -ne $true) { throw "Hardware proof does not contain DOM input evidence" }

$adapter = [string]$proof.runtime.adapter
$device = [string]$proof.hardware.active_device.deviceString
$driverVendor = [string]$proof.hardware.active_device.driverVendor
$driverVersion = [string]$proof.hardware.active_device.driverVersion
if ([string]::IsNullOrWhiteSpace($adapter) -or [string]::IsNullOrWhiteSpace($device) -or
    [string]::IsNullOrWhiteSpace($driverVendor) -or [string]::IsNullOrWhiteSpace($driverVersion)) {
    throw "Hardware proof is missing adapter or driver information"
}

$capturedAt = [DateTimeOffset]::Parse([string]$proof.captured_at_utc)
$startedAt = [DateTimeOffset]::Parse([string]$proof.r3_complexity_matrix.started_at_utc)
$finishedAt = [DateTimeOffset]::Parse([string]$proof.r3_complexity_matrix.finished_at_utc)
$command = [string]$proof.r3_complexity_matrix.command
if ($finishedAt -lt $startedAt -or [string]::IsNullOrWhiteSpace($command)) { throw "Hardware proof is missing a valid execution interval or command" }
$age = [DateTimeOffset]::UtcNow - $capturedAt.ToUniversalTime()
if ($age.TotalMinutes -lt -5 -or $age.TotalHours -gt $MaximumAgeHours) {
    throw "Hardware proof is not fresh enough for this phase-gate dispatch"
}

$pixelRelative = ([string]$proof.pixel_readback_artifact).Replace("\", "/")
if ($pixelRelative -notmatch "^docs/verification/.+\.json$") { throw "Pixel readback artifact path is invalid" }
$pixelFull = [IO.Path]::GetFullPath((Join-Path $repoRoot $pixelRelative))
if (-not $pixelFull.StartsWith($verificationRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not (Test-Path -LiteralPath $pixelFull -PathType Leaf)) {
    throw "Pixel readback artifact is missing or outside docs/verification"
}
$null = & git -C $repoRoot ls-files --error-unmatch -- $pixelRelative 2>$null
if ($LASTEXITCODE -ne 0) { throw "Pixel readback artifact must be tracked by Git" }
$pixelProof = Get-Content -Raw -LiteralPath $pixelFull | ConvertFrom-Json
if ($pixelProof.kind -ne "actual-webgpu-texture-readback" -or $pixelProof.all_passed -ne $true) {
    throw "Pixel readback artifact is not a passing actual-WebGPU readback"
}

[pscustomobject]@{
    evidence_path = $relative
    evidence_sha256 = $actualHash
    captured_at_utc = $capturedAt.ToUniversalTime().ToString("o")
    command = $command
    actual_webgpu = $true
    mock_or_fallback = $false
    pixel_readback = $true
    dom_input = $true
    adapter = $adapter
    device = $device
    driver_vendor = $driverVendor
    driver_version = $driverVersion
    all_passed = $true
} | ConvertTo-Json -Depth 4
