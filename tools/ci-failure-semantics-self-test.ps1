$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("editorm-failure-semantics-" + [Guid]::NewGuid().ToString("N"))
$fixturePath = Join-Path $testRoot "fail-once.ps1"
$counterPath = Join-Path $testRoot "invocations.txt"
$currentPowerShell = (Get-Process -Id $PID).Path

try {
    [void](New-Item -ItemType Directory -Path $testRoot)
    $fixture = @'
param([Parameter(Mandatory = $true)][string]$CounterPath)
$count = if (Test-Path -LiteralPath $CounterPath) { [int](Get-Content -Raw -LiteralPath $CounterPath) } else { 0 }
[IO.File]::WriteAllText($CounterPath, ([string]($count + 1)), [Text.UTF8Encoding]::new($false))
exit 23
'@
    [IO.File]::WriteAllText($fixturePath, $fixture, [Text.UTF8Encoding]::new($false))

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $null = & $currentPowerShell -NoProfile -File $fixturePath -CounterPath $counterPath 2>&1
    $failureExit = $LASTEXITCODE
    $ErrorActionPreference = $previousPreference

    if ($failureExit -ne 23) { throw "Synthetic failure exit code was not preserved." }
    $invocations = [int](Get-Content -Raw -LiteralPath $counterPath)
    if ($invocations -ne 1) { throw "Synthetic failure command was automatically retried." }

    $prWorkflow = Get-Content -Raw -LiteralPath (Join-Path $repoRoot ".github/workflows/pr-fast.yml")
    $gateWorkflow = Get-Content -Raw -LiteralPath (Join-Path $repoRoot ".github/workflows/phase-gate.yml")
    foreach ($workflow in @($prWorkflow, $gateWorkflow)) {
        if ($workflow -match 'ci-cargo\.ps1' -or
            $workflow -match '(?s)catch\s*\{.*build-phase0e-wasm' -or
            $workflow -match 'clean-cache fallback' -or
            $workflow -match 'Remove-Item[^\r\n]*target') {
            throw "A workflow still contains automatic failure-retry or target cleanup logic."
        }
    }

    foreach ($workflow in @($prWorkflow, $gateWorkflow)) {
        if ([regex]::Matches($workflow, '(?m)^\s*run:\s*cargo clippy --workspace --all-targets -- -D warnings\s*$').Count -ne 1) {
            throw "A workflow does not run Clippy directly exactly once."
        }
        if ([regex]::Matches($workflow, '(?m)^\s*run:\s*cargo test --workspace --all-targets\s*$').Count -ne 1) {
            throw "A workflow does not run workspace tests directly exactly once."
        }
        if ([regex]::Matches($workflow, '(?m)^\s*(run:\s*)?\./tools/build-phase0e-wasm\.ps1 -OutDir target/ci-wasm-pkg\s*$').Count -ne 1) {
            throw "A workflow does not run the pinned WASM build exactly once."
        }
    }

    [pscustomobject]@{
        synthetic_exit_code = $failureExit
        synthetic_invocations = $invocations
        automatic_retry = $false
        workflow_static_checks = $true
        all_passed = $true
    } | ConvertTo-Json
}
finally {
    $resolvedTemp = [IO.Path]::GetFullPath($testRoot)
    $systemTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ($resolvedTemp.StartsWith($systemTemp, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedTemp) -like "editorm-failure-semantics-*") {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

exit 0
