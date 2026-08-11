param(
    [string]$MetricsPath = "docs/PHASE_0C_METRICS.json",
    [string]$VerificationPath = "docs/verification/PHASE_0C_VERIFICATION.txt"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$metricsFull = Join-Path $repoRoot $MetricsPath
$verificationFull = Join-Path $repoRoot $VerificationPath
$verificationDirectory = Split-Path -Parent $verificationFull
New-Item -ItemType Directory -Force -Path $verificationDirectory | Out-Null

$started = Get-Date
$command = "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 run --release -p visual_authoring_runtime --bin phase0c_proof -- --metrics-json $MetricsPath"
$previousErrorAction = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$output = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0c_proof -- --metrics-json $metricsFull 2>&1
$exitCode = $LASTEXITCODE
$ErrorActionPreference = $previousErrorAction
$finished = Get-Date

$lines = @(
    "Phase 0C verification raw output"
    "started=$($started.ToString('o'))"
    "finished=$($finished.ToString('o'))"
    "command=$command"
    "exit_code=$exitCode"
    ""
) + ($output | ForEach-Object { $_.ToString() })
[IO.File]::WriteAllLines($verificationFull, $lines, [Text.UTF8Encoding]::new($false))

$output | ForEach-Object { Write-Output $_ }
if ($exitCode -ne 0) {
    throw "Phase 0C proof runner failed with exit code $exitCode. See $verificationFull"
}

$metrics = Get-Content -Raw -LiteralPath $metricsFull | ConvertFrom-Json
if (-not $metrics.all_passed) {
    throw "Phase 0C proof metrics did not satisfy all structural acceptance checks."
}

Write-Output "verification_log=$verificationFull"
Write-Output "metrics_json=$metricsFull"
