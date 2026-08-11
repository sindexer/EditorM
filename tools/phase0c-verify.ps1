$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$verificationPath = Join-Path $repoRoot "docs/verification/PHASE_0C_VERIFICATION.txt"
$verificationDirectory = Split-Path -Parent $verificationPath
New-Item -ItemType Directory -Force -Path $verificationDirectory | Out-Null
if (Test-Path -LiteralPath $verificationPath) {
    Remove-Item -LiteralPath $verificationPath -Force
}

$commands = @(
    @("fmt", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check"),
    @("clippy", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings"),
    @("build", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets"),
    @("test", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets"),
    @("doctest", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace"),
    @("release", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets"),
    @("tree", "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 tree --workspace")
)

$sections = [Collections.Generic.List[string]]::new()
$failed = $false
$previousErrorAction = $ErrorActionPreference
$ErrorActionPreference = "Continue"
foreach ($entry in $commands) {
    $label = $entry[0]
    $command = $entry[1]
    $started = Get-Date
    $output = & powershell -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1
    $exitCode = $LASTEXITCODE
    $finished = Get-Date
    $sections.Add("=== $label ===")
    $sections.Add("started=$($started.ToString('o'))")
    $sections.Add("finished=$($finished.ToString('o'))")
    $sections.Add("command=$command")
    $sections.Add("exit_code=$exitCode")
    $sections.Add("")
    foreach ($line in $output) {
        $sections.Add($line.ToString())
    }
    $sections.Add("")
    Write-Output "$label exit_code=$exitCode"
    if ($exitCode -ne 0) {
        $failed = $true
        break
    }
}

if (-not $failed) {
    $proofOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "phase0c-proof.ps1") 2>&1
    $proofExitCode = $LASTEXITCODE
    $proofRaw = if (Test-Path -LiteralPath $verificationPath) {
        Get-Content -LiteralPath $verificationPath
    } else {
        $proofOutput | ForEach-Object { $_.ToString() }
    }
    $sections.Add("=== proof ===")
    foreach ($line in $proofRaw) {
        $sections.Add($line)
    }
    $sections.Add("")
    Write-Output "proof exit_code=$proofExitCode"
    if ($proofExitCode -ne 0) {
        $failed = $true
    }
}
$ErrorActionPreference = $previousErrorAction

$header = @(
    "Phase 0C mandated verification raw output"
    "generated=$((Get-Date).ToString('o'))"
    ""
)
[IO.File]::WriteAllLines(
    $verificationPath,
    $header + $sections,
    [Text.UTF8Encoding]::new($false)
)

if ($failed) {
    throw "Phase 0C verification failed. See $verificationPath"
}
Write-Output "verification_log=$verificationPath"
