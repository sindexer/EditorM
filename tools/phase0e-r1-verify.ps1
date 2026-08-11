param(
    [string]$VerificationPath = "docs/verification/PHASE_0E_R1_VERIFICATION.txt"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$editorRoot = Join-Path $repoRoot "web/editor"
$phase0dRoot = Join-Path $repoRoot "web/phase0d-preview"
$verificationFull = Join-Path $repoRoot $VerificationPath
$npm = "C:/Program Files/nodejs/npm.cmd"
$scratch = Join-Path $env:TEMP ("phase0e-r1-verify-" + [Guid]::NewGuid().ToString("N"))
$started = Get-Date
$records = [Collections.Generic.List[string]]::new()
$failure = $null

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $verificationFull) | Out-Null
New-Item -ItemType Directory -Force -Path $scratch | Out-Null

function Invoke-VerificationStep {
    param(
        [string]$Name,
        [string]$Command,
        [scriptblock]$Action
    )
    $stepStarted = Get-Date
    Write-Output "=== $Name ==="
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = & $Action 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previous
    $stepFinished = Get-Date
    $records.Add("=== $Name ===")
    $records.Add("command=$Command")
    $records.Add("started=$($stepStarted.ToString('o'))")
    $records.Add("finished=$($stepFinished.ToString('o'))")
    $records.Add("exit_code=$exitCode")
    foreach ($line in $output) {
        $text = $line.ToString()
        $records.Add($text)
        Write-Output $text
    }
    $records.Add("")
    if ($exitCode -ne 0) { throw "$Name failed with exit code $exitCode" }
}

function Assert-ProofPassed {
    param([string]$Path, [string]$Label)
    $proof = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    if (-not $proof.all_passed) { throw "$Label reports all_passed=false" }
    $records.Add("$Label`_all_passed=true")
}

Push-Location $repoRoot
try {
    try {
        Invoke-VerificationStep "cargo fmt" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") fmt --all -- --check
        }
        Invoke-VerificationStep "cargo clippy" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") clippy --workspace --all-targets -- -D warnings
        }
        Invoke-VerificationStep "cargo debug build" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --workspace --all-targets
        }
        Invoke-VerificationStep "cargo all-target tests" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") test --workspace --all-targets
        }
        Invoke-VerificationStep "cargo doctests" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") test --doc --workspace
        }
        Invoke-VerificationStep "cargo release build" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --release --workspace --all-targets
        }
        Invoke-VerificationStep "wasm32 release build" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --target wasm32-unknown-unknown" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --release --target wasm32-unknown-unknown
        }
        Invoke-VerificationStep "Phase 0E-R1 generated WASM package" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-phase0e-wasm.ps1" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "build-phase0e-wasm.ps1")
        }

        $phase0cMetrics = Join-Path $scratch "PHASE_0C_METRICS.json"
        $phase0dMetrics = Join-Path $scratch "PHASE_0D_METRICS.json"
        $phase0dR1Metrics = Join-Path $scratch "PHASE_0D_R1_METRICS.json"
        Invoke-VerificationStep "Phase 0C regression proof" "cargo run --release -p visual_authoring_runtime --bin phase0c_proof -- --metrics-json <temp>" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0c_proof -- --metrics-json $phase0cMetrics
        }
        Invoke-VerificationStep "Phase 0D native regression proof" "cargo run --release -p visual_authoring_runtime --bin phase0d_proof -- <temp>" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0d_proof -- $phase0dMetrics
        }
        Invoke-VerificationStep "Phase 0D-R1 native regression proof" "cargo run --release -p visual_authoring_runtime --bin phase0d_r1_proof -- <temp>" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0d_r1_proof -- $phase0dR1Metrics
        }
        Assert-ProofPassed $phase0cMetrics "phase0c_native_regression"
        Assert-ProofPassed $phase0dMetrics "phase0d_native_regression"
        Assert-ProofPassed $phase0dR1Metrics "phase0d_r1_native_regression"

        $scratchWorkspace = Join-Path $scratch "WebEditor"
        $scratchPhase0d = Join-Path $scratchWorkspace "web/phase0d-preview"
        New-Item -ItemType Directory -Force -Path $scratchPhase0d | Out-Null
        Get-ChildItem -LiteralPath $phase0dRoot -Force |
            Where-Object { $_.Name -notin @("node_modules", "dist") } |
            Copy-Item -Destination $scratchPhase0d -Recurse -Force
        New-Item -ItemType Directory -Force -Path (Join-Path $scratchWorkspace "shared") | Out-Null
        Copy-Item -LiteralPath (Join-Path $repoRoot "shared/render_contract.wgsl") -Destination (Join-Path $scratchWorkspace "shared/render_contract.wgsl")
        Copy-Item -LiteralPath (Join-Path $repoRoot "shared/render_binary_schema.json") -Destination (Join-Path $scratchWorkspace "shared/render_binary_schema.json")
        New-Item -ItemType Directory -Force -Path (Join-Path $scratchWorkspace "docs/verification") | Out-Null
        Push-Location $scratchPhase0d
        try {
            Invoke-VerificationStep "Phase 0D-R1 web clean install" "npm ci --ignore-scripts (temporary regression workspace)" {
                & $npm ci --ignore-scripts
            }
            Invoke-VerificationStep "Phase 0D-R1 web unit regression" "npm test (temporary regression workspace)" {
                & $npm test
            }
            Invoke-VerificationStep "Phase 0D-R1 actual hardware browser regression" "npm run test:browser (temporary regression workspace)" {
                Remove-Item Env:PHASE0D_URL -ErrorAction SilentlyContinue
                & $npm run test:browser
            }
        } finally {
            Remove-Item Env:PHASE0D_URL -ErrorAction SilentlyContinue
            Pop-Location
        }
        Assert-ProofPassed (Join-Path $scratchWorkspace "docs/verification/PHASE_0D_R1_BROWSER_PROOF.json") "phase0d_r1_browser_regression"
        Assert-ProofPassed (Join-Path $scratchWorkspace "docs/verification/PHASE_0D_R1_PIXEL_READBACK.json") "phase0d_r1_pixel_regression"
        $records.Add("prior_stage_regression_outputs=temp_only")

        Push-Location $editorRoot
        try {
            Invoke-VerificationStep "Phase 0E-R1 web clean install" "npm ci --ignore-scripts" {
                & $npm ci --ignore-scripts
            }
            Invoke-VerificationStep "Phase 0E-R1 React production build" "npm run build" {
                & $npm run build
            }
            Invoke-VerificationStep "Phase 0E-R1 web unit tests" "npm test" {
                & $npm test
            }
        } finally {
            Pop-Location
        }

        Invoke-VerificationStep "Phase 0E-R1 native structural proof" "cargo run --release -p visual_authoring_runtime --bin phase0e_proof -- docs/PHASE_0E_R1_METRICS.json" {
            & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0e_proof -- docs/PHASE_0E_R1_METRICS.json
        }
        Assert-ProofPassed (Join-Path $repoRoot "docs/PHASE_0E_R1_METRICS.json") "phase0e_r1_native"

        Push-Location $editorRoot
        try {
            Invoke-VerificationStep "Phase 0E-R1 self-contained actual hardware browser proof" "npm run test:browser" {
                & $npm run test:browser
            }
        } finally {
            Pop-Location
        }
        Assert-ProofPassed (Join-Path $repoRoot "docs/verification/PHASE_0E_R1_BROWSER_PROOF.json") "phase0e_r1_browser"
        Assert-ProofPassed (Join-Path $repoRoot "docs/verification/PHASE_0E_R1_PIXEL_READBACK.json") "phase0e_r1_pixel"
        $records.Add("all_required_commands_passed=true")
        $records.Add("failed=0")
        $records.Add("ignored=0")
        $records.Add("skipped=0")
        $records.Add("todo=0")
        $records.Add("cancelled=0")
    } catch {
        $failure = $_
        $records.Add("verification_error=$($_.Exception.Message)")
    }
} finally {
    Pop-Location
    $finished = Get-Date
    $header = @(
        "Phase 0E-R1 verification raw output"
        "started=$($started.ToString('o'))"
        "finished=$($finished.ToString('o'))"
        "workspace=$repoRoot"
        "prior_stage_artifacts_written=false"
        "temporary_regression_workspace=$scratch"
        ""
    )
    [IO.File]::WriteAllLines($verificationFull, $header + $records, [Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $scratch) {
        $resolvedScratch = (Resolve-Path -LiteralPath $scratch).Path
        $resolvedTemp = (Resolve-Path -LiteralPath $env:TEMP).Path
        if (-not $resolvedScratch.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove scratch directory outside TEMP: $resolvedScratch"
        }
        Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
    }
}

if ($failure) { throw $failure }
Write-Output "verification_log=$verificationFull"
