$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/editor"
$verificationRoot = Join-Path $repoRoot "docs/verification"
$verificationLog = Join-Path $verificationRoot "PHASE_0E_R2_VERIFICATION.txt"
$nativeLog = Join-Path $verificationRoot "PHASE_0E_R2_NATIVE_STRUCTURAL_OUTPUT.txt"
$wasmLog = Join-Path $verificationRoot "PHASE_0E_R2_DIRECT_WASM_STRUCTURAL_OUTPUT.txt"
$reactLog = Join-Path $verificationRoot "PHASE_0E_R2_REACT_PROJECTION_OUTPUT.txt"
$utf8 = [Text.UTF8Encoding]::new($false)
$transcript = [Collections.Generic.List[string]]::new()

function Invoke-VerificationStep {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [string]$DedicatedLog
    )

    $command = "$Executable $($Arguments -join ' ')"
    $header = @(
        "",
        "=== $Name ===",
        "working_directory=$WorkingDirectory",
        "command=$command",
        "started_at_utc=$([DateTime]::UtcNow.ToString('o'))"
    )
    $header | ForEach-Object { $transcript.Add($_); Write-Host $_ }

    Push-Location $WorkingDirectory
    try {
        $oldPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        $output = @(& $Executable @Arguments 2>&1 | ForEach-Object { $_.ToString() })
        $exitCode = $LASTEXITCODE
        $ErrorActionPreference = $oldPreference
    } finally {
        Pop-Location
    }

    $output | ForEach-Object { $transcript.Add($_); Write-Host $_ }
    $footer = @(
        "exit_code=$exitCode",
        "finished_at_utc=$([DateTime]::UtcNow.ToString('o'))"
    )
    $footer | ForEach-Object { $transcript.Add($_); Write-Host $_ }

    if ($DedicatedLog) {
        [IO.File]::WriteAllLines($DedicatedLog, @($header + $output + $footer), $utf8)
    }
    [IO.File]::WriteAllLines($verificationLog, $transcript, $utf8)

    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode"
    }
}

New-Item -ItemType Directory -Force -Path $verificationRoot | Out-Null
$transcript.Add("Phase 0E-R2 final verification")
$transcript.Add("generated_at_utc=$([DateTime]::UtcNow.ToString('o'))")
$transcript.Add("prior_evidence_policy=R2-only outputs; Phase 0D/0D-R1-P1/0E/0E-R1 evidence is not written")

$cargo = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "tools/cargo.ps1")
Invoke-VerificationStep "Rust fmt" $repoRoot "powershell" ($cargo + @("fmt", "--all", "--", "--check"))
Invoke-VerificationStep "Rust clippy" $repoRoot "powershell" ($cargo + @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings"))
Invoke-VerificationStep "Rust debug build" $repoRoot "powershell" ($cargo + @("build", "--workspace", "--all-targets"))
Invoke-VerificationStep "Rust debug all-target tests" $repoRoot "powershell" ($cargo + @("test", "--workspace", "--all-targets"))
Invoke-VerificationStep "Rust doctests" $repoRoot "powershell" ($cargo + @("test", "--doc", "--workspace"))
Invoke-VerificationStep "Rust release build" $repoRoot "powershell" ($cargo + @("build", "--release", "--workspace", "--all-targets"))
Invoke-VerificationStep "Rust release WASM build" $repoRoot "powershell" ($cargo + @("build", "--release", "--target", "wasm32-unknown-unknown"))
Invoke-VerificationStep "WASM bindgen packaging" $repoRoot "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "tools/build-phase0e-wasm.ps1")
Invoke-VerificationStep "Native release structural benchmark" $repoRoot "powershell" ($cargo + @("test", "--release", "-p", "visual_authoring_runtime", "--test", "phase0e_r2_bounded", "--", "--nocapture")) $nativeLog
Invoke-VerificationStep "Direct EngineHost release structural benchmark" $repoRoot "powershell" ($cargo + @("test", "--release", "-p", "visual_authoring_wasm_bridge", "phase0e_r2_direct_engine_host", "--", "--nocapture")) $wasmLog
Invoke-VerificationStep "Clean frontend dependency install" $webRoot "npm.cmd" @("ci", "--ignore-scripts")
Invoke-VerificationStep "Frontend production build" $webRoot "npm.cmd" @("run", "build")
Invoke-VerificationStep "React ProjectionStore tests" $webRoot "npm.cmd" @("test") $reactLog
Invoke-VerificationStep "Actual Chrome R2 browser proof" $webRoot "npm.cmd" @("run", "test:browser:r2")

$transcript.Add("")
$transcript.Add("FINAL_STATUS=PASS")
$transcript.Add("completed_at_utc=$([DateTime]::UtcNow.ToString('o'))")
[IO.File]::WriteAllLines($verificationLog, $transcript, $utf8)
Write-Host "verification_log=$verificationLog"
Write-Host "native_log=$nativeLog"
Write-Host "wasm_log=$wasmLog"
Write-Host "react_log=$reactLog"
