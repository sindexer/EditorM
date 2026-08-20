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
$phaseProperty = $proof.PSObject.Properties["phase"]
$isPhase1A = $null -ne $phaseProperty -and [string]$phaseProperty.Value -eq "1A"

if ($isPhase1A) {
    if ($proof.initial.actual_webgpu -ne $true -or $proof.checks.actual_adapter_and_device -ne $true) {
        throw "Phase 1A evidence does not prove actual WebGPU hardware use"
    }
    if ($proof.gpu.backend -notmatch "browser-webgpu") { throw "Phase 1A evidence backend is not browser WebGPU" }
    if ($proof.browser_mode -notmatch "no mock" -or $proof.browser_mode -notmatch "no Canvas2D fallback") {
        throw "Phase 1A evidence does not explicitly exclude mock and Canvas2D fallback"
    }
    if ($proof.max_fallback_rebuild_count_seen -ne 0 -or $proof.checks.fallback_rebuild_zero -ne $true) {
        throw "Phase 1A evidence reports a fallback or rebuild"
    }
    if ($proof.all_passed -ne $true -or $proof.checks.actual_pixel_readback -ne $true) {
        throw "Phase 1A hardware proof did not pass all checks"
    }
    if ($proof.checks.frame_4k_created_selected_undo_redo -ne $true -or $proof.checks.direct_move_resize_undo_redo -ne $true) {
        throw "Phase 1A hardware proof does not contain DOM input evidence"
    }

    $requiredPhase1AAssertions = @(
        "affine_semantic_parity",
        "srgb_render_view_active",
        "expected_color_contract_match",
        "halo_free_black_background",
        "halo_free_white_background",
        "fill_stroke_boundary_has_no_alpha_dip",
        "ellipse_stroke_width_uniform",
        "render_hit_test_overlay_parity"
    )
    foreach ($assertion in $requiredPhase1AAssertions) {
        $property = $proof.checks.PSObject.Properties[$assertion]
        if ($null -eq $property -or $property.Value -ne $true) {
            throw "Phase 1A hardware proof assertion failed or is missing: $assertion"
        }
    }
    $surfaceBaseFormat = [string]$proof.gpu.surface_base_format
    $pipelineViewFormat = [string]$proof.gpu.pipeline_view_format
    $readbackViewFormat = [string]$proof.gpu.readback_view_format
    if ($surfaceBaseFormat -notmatch "^(bgra|rgba)8unorm$" -or
        $pipelineViewFormat -ne "$surfaceBaseFormat-srgb" -or
        $readbackViewFormat -ne $pipelineViewFormat) {
        throw "Phase 1A hardware proof does not use a compatible sRGB render/readback view"
    }

    $adapter = [string]$proof.gpu.adapter
    $device = [string]$proof.gpu.active_device.deviceString
    $driverVendor = [string]$proof.gpu.active_device.driverVendor
    $driverVersion = [string]$proof.gpu.active_device.driverVersion
    $capturedAt = [DateTimeOffset]::Parse([string]$proof.captured_at_utc)
    $startedAt = [DateTimeOffset]::Parse([string]$proof.execution.started_at_utc)
    $finishedAt = [DateTimeOffset]::Parse([string]$proof.execution.finished_at_utc)
    $command = [string]$proof.execution.command
}
else {
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
    $capturedAt = [DateTimeOffset]::Parse([string]$proof.captured_at_utc)
    $startedAt = [DateTimeOffset]::Parse([string]$proof.r3_complexity_matrix.started_at_utc)
    $finishedAt = [DateTimeOffset]::Parse([string]$proof.r3_complexity_matrix.finished_at_utc)
    $command = [string]$proof.r3_complexity_matrix.command
}

if ([string]::IsNullOrWhiteSpace($adapter) -or [string]::IsNullOrWhiteSpace($device) -or
    [string]::IsNullOrWhiteSpace($driverVendor) -or [string]::IsNullOrWhiteSpace($driverVersion)) {
    throw "Hardware proof is missing adapter or driver information"
}
if ($finishedAt -lt $startedAt -or [string]::IsNullOrWhiteSpace($command)) {
    throw "Hardware proof is missing a valid execution interval or command"
}
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
if ($isPhase1A) {
    if ($pixelProof.proof_kind -ne "actual-hardware-webgpu-pixel-readback" -or $pixelProof.all_passed -ne $true) {
        throw "Phase 1A pixel artifact is not a passing actual-WebGPU readback"
    }
    if ([string]$pixelProof.surface_base_format -ne $surfaceBaseFormat -or
        [string]$pixelProof.pipeline_view_format -ne $pipelineViewFormat -or
        [string]$pixelProof.readback_view_format -ne $readbackViewFormat) {
        throw "Phase 1A pixel proof sRGB formats do not match the browser proof"
    }
    foreach ($assertion in $requiredPhase1AAssertions) {
        $property = $pixelProof.assertions.PSObject.Properties[$assertion]
        if ($null -eq $property -or $property.Value -ne $true) {
            throw "Phase 1A pixel proof assertion failed or is missing: $assertion"
        }
    }
    if ([string]$pixelProof.ellipse_stroke_measurement_method -ne "integrated connected linear coverage over an isolated black backdrop" -or
        [double]$pixelProof.ellipse_stroke_tolerance_physical -ne 1.0 -or
        [double]$pixelProof.ellipse_stroke_max_error_physical -gt 1.0) {
        throw "Phase 1A ellipse stroke proof must use integrated coverage with a 1 physical pixel tolerance"
    }
    $measurements = @($pixelProof.ellipse_stroke_measurements)
    if ([int]$pixelProof.ellipse_stroke_measurement_count -ne 48 -or $measurements.Count -ne 48) {
        throw "Phase 1A pixel proof must contain all 48 ellipse DPR/zoom stroke measurements"
    }
    foreach ($measurement in $measurements) {
        if ($measurement.passed -ne $true -or
            $measurement.major.clipping_free -ne $true -or
            $measurement.minor.clipping_free -ne $true -or
            [double]$measurement.major.absolute_error_physical -gt [double]$measurement.major.tolerance_physical -or
            [double]$measurement.minor.absolute_error_physical -gt [double]$measurement.minor.tolerance_physical -or
            [double]$measurement.major.absolute_error_local -gt [double]$measurement.major.tolerance_local -or
            [double]$measurement.minor.absolute_error_local -gt [double]$measurement.minor.tolerance_local) {
            throw "Phase 1A ellipse stroke measurement failed or exceeded tolerance"
        }
    }
    if (@($pixelProof.color_contract_cases).Count -ne 3 -or
        [double]$pixelProof.color_contract_max_channel_error -gt [double]$pixelProof.color_channel_tolerance_linear) {
        throw "Phase 1A linear premultiplied color contract evidence is incomplete or outside tolerance"
    }
    if ($pixelProof.affine_parity.m12_not_equal_m21 -ne $true -or
        $pixelProof.affine_parity.actual_click_selects_shape -ne $true -or
        $pixelProof.affine_parity.transposed_click_does_not_select_shape -ne $true) {
        throw "Phase 1A affine render/hit-test/overlay parity evidence is incomplete"
    }
}
elseif ($pixelProof.kind -ne "actual-webgpu-texture-readback" -or $pixelProof.all_passed -ne $true) {
    throw "Pixel readback artifact is not a passing actual-WebGPU readback"
}

[pscustomobject]@{
    evidence_path = $relative
    evidence_sha256 = $actualHash
    phase = if ($isPhase1A) { "1A" } else { "legacy" }
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