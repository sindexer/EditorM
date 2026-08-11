param(
    [string]$VerificationPath = "docs/verification/PHASE_0D_VERIFICATION.txt",
    [int]$BrowserPort = 4176
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$webRoot = Join-Path $repoRoot "web/phase0d-preview"
$verificationFull = Join-Path $repoRoot $VerificationPath
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $verificationFull) | Out-Null
$npm = "C:/Program Files/nodejs/npm.cmd"
$node = "C:/Program Files/nodejs/node.exe"
$started = Get-Date
$records = [Collections.Generic.List[string]]::new()
$server = $null

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
    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode"
    }
}

Push-Location $repoRoot
try {
    Invoke-VerificationStep "cargo fmt" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") fmt --all -- --check
    }
    Invoke-VerificationStep "cargo clippy" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") clippy --workspace --all-targets -- -D warnings
    }
    Invoke-VerificationStep "cargo build" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --workspace --all-targets
    }
    Invoke-VerificationStep "cargo test" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") test --workspace --all-targets
    }
    Invoke-VerificationStep "cargo doctest" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") test --doc --workspace
    }
    Invoke-VerificationStep "cargo release build" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --release --workspace --all-targets
    }
    Invoke-VerificationStep "cargo tree" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 tree --workspace" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") tree --workspace
    }
    Invoke-VerificationStep "wasm32 release build" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --target wasm32-unknown-unknown" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") build --release --target wasm32-unknown-unknown
    }
    Invoke-VerificationStep "Phase 0C regression proof" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0c-proof.ps1" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "phase0c-proof.ps1")
    }
    Invoke-VerificationStep "Phase 0D native wgpu proof" "powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 run --release -p visual_authoring_runtime --bin phase0d_proof -- docs/PHASE_0D_METRICS.json" {
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "cargo.ps1") run --release -p visual_authoring_runtime --bin phase0d_proof -- docs/PHASE_0D_METRICS.json
    }

    Push-Location $webRoot
    try {
        Invoke-VerificationStep "web clean install" "npm ci --ignore-scripts" {
            & $npm ci --ignore-scripts
        }
        Invoke-VerificationStep "web build" "npm run build" {
            & $npm run build
        }
        Invoke-VerificationStep "web unit tests" "npm test" {
            & $npm test
        }
        $stdout = Join-Path $env:TEMP "phase0d-verify-$BrowserPort.stdout.log"
        $stderr = Join-Path $env:TEMP "phase0d-verify-$BrowserPort.stderr.log"
        $server = Start-Process -FilePath $node -ArgumentList "server.mjs","--port=$BrowserPort" -WorkingDirectory $webRoot -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
        $health = "http://127.0.0.1:$BrowserPort/__health"
        $deadline = (Get-Date).AddSeconds(10)
        do {
            Start-Sleep -Milliseconds 200
            try { $ready = (Invoke-RestMethod -Uri $health -TimeoutSec 2).ok } catch { $ready = $false }
        } while (-not $ready -and (Get-Date) -lt $deadline)
        if (-not $ready) { throw "Browser test server did not become ready. See $stderr" }
        $env:PHASE0D_URL = "http://127.0.0.1:$BrowserPort/"
        Invoke-VerificationStep "actual Chrome WebGPU browser tests" "npm run test:browser" {
            & $npm run test:browser
        }
    } finally {
        Remove-Item Env:PHASE0D_URL -ErrorAction SilentlyContinue
        Pop-Location
    }

    $native = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "docs/PHASE_0D_METRICS.json") | ConvertFrom-Json
    $browser = Get-Content -Raw -LiteralPath (Join-Path $repoRoot "docs/verification/PHASE_0D_R1_BROWSER_PROOF.json") | ConvertFrom-Json
    if (-not $native.all_passed) { throw "Native Phase 0D metrics report failure" }
    if (-not $browser.all_passed) { throw "Browser Phase 0D proof reports failure" }
    $records.Add("all_required_commands_passed=true")
    $records.Add("native_proof_all_passed=true")
    $records.Add("browser_proof_all_passed=true")
} finally {
    if ($server -and -not $server.HasExited) { Stop-Process -Id $server.Id }
    $finished = Get-Date
    $header = @(
        "Phase 0D verification raw output"
        "started=$($started.ToString('o'))"
        "finished=$($finished.ToString('o'))"
        "workspace=$repoRoot"
        ""
    )
    [IO.File]::WriteAllLines($verificationFull, $header + $records, [Text.UTF8Encoding]::new($false))
    Pop-Location
}
Write-Output "verification_log=$verificationFull"