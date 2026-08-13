$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$outputPath = Join-Path $repoRoot "docs/verification/PHASE_0E_R1_GROUP_TEST_OUTPUT.txt"
$started = Get-Date

Push-Location $repoRoot
try {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") test --release -p visual_authoring_runtime --test phase0e_r1_structure -- --nocapture 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previous
} finally {
    Pop-Location
}

$lines = @(
    "Phase 0E-R1 raw structural test output"
    "started=$($started.ToString('o'))"
    "finished=$((Get-Date).ToString('o'))"
    "command=powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --release -p visual_authoring_runtime --test phase0e_r1_structure -- --nocapture"
    "exit_code=$exitCode"
    ""
) + @($output | ForEach-Object { $_.ToString() })
[IO.File]::WriteAllLines($outputPath, $lines, [Text.UTF8Encoding]::new($false))
$output | ForEach-Object { Write-Output $_.ToString() }
if ($exitCode -ne 0) { throw "Phase 0E-R1 structural proof failed with exit code $exitCode" }
Write-Output "structural_proof=$outputPath"

