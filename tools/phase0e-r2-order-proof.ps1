$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/editor"
$outputPath = Join-Path $repoRoot "docs/verification/PHASE_0E_R2_ORDER_ACCURACY_OUTPUT.txt"
$utf8 = [Text.UTF8Encoding]::new($false)

function Invoke-Captured([string]$WorkingDirectory, [string]$Executable, [string[]]$Arguments) {
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
    if ($exitCode -ne 0) { throw "$Executable $($Arguments -join ' ') failed with $exitCode" }
    return $output
}

$rust = Invoke-Captured $repoRoot "powershell" @(
    "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "tools/cargo.ps1",
    "test", "-p", "visual_authoring_runtime", "--test", "phase0e_r2_order_accuracy", "--", "--nocapture"
)
$react = Invoke-Captured $webRoot "npm.cmd" @("test")

$rustMarker = @($rust | Where-Object { $_ -match "PHASE0E_R2_ORDER_JSON=(\{.+\})" })
$reactMarker = @($react | Where-Object { $_ -match "PHASE0E_R2_REACT_ORDER_JSON=(\{.+\})" })
if ($rustMarker.Count -ne 1) { throw "Expected one Rust order JSON marker, got $($rustMarker.Count)" }
if ($reactMarker.Count -ne 1) { throw "Expected one React order JSON marker, got $($reactMarker.Count)" }

$rustJson = ($rustMarker[0] -replace "^.*PHASE0E_R2_ORDER_JSON=", "") | ConvertFrom-Json
$reactJson = ($reactMarker[0] -replace "^.*PHASE0E_R2_REACT_ORDER_JSON=", "") | ConvertFrom-Json
if (-not $rustJson.all_passed -or -not $reactJson.all_passed) { throw "Order proof marker did not pass" }

$lines = [Collections.Generic.List[string]]::new()
$lines.Add("Phase 0E-R2 exact order proof")
$lines.Add("captured_at_utc=$([DateTime]::UtcNow.ToString('o'))")
$lines.Add("rust_command=powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test -p visual_authoring_runtime --test phase0e_r2_order_accuracy -- --nocapture")
$rust | ForEach-Object { $lines.Add($_) }
$lines.Add("")
$lines.Add("react_command=npm test")
$react | ForEach-Object { $lines.Add($_) }
$lines.Add("")
$lines.Add("FINAL_STATUS=PASS")
[IO.File]::WriteAllLines($outputPath, $lines, $utf8)
Write-Host "order_accuracy=$outputPath"
Write-Host "rust_all_passed=$($rustJson.all_passed)"
Write-Host "react_all_passed=$($reactJson.all_passed)"
