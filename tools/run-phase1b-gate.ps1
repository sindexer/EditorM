[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$editorRoot = Join-Path $repoRoot "web\editor"
$manifestPath = Join-Path $repoRoot "docs\verification\PHASE_1B_GATE_RUN.json"
$finalizerPath = Join-Path $editorRoot "scripts\finalize-gate-phase1b.mjs"
$expectedBranch = "main"
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$manifest = $null
$activeStep = "startup"

function Get-UtcNow {
    return [DateTime]::UtcNow.ToString("o")
}

function Write-Manifest {
    if ($null -eq $script:manifest) { return }
    $json = $script:manifest | ConvertTo-Json -Depth 16
    [System.IO.File]::WriteAllText($script:manifestPath, "$json`n", $script:utf8NoBom)
}

function Get-VersionRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [string[]]$Arguments = @()
    )
    try {
        $output = & $Executable @Arguments 2>&1
        $exitCode = $LASTEXITCODE
        return [ordered]@{
            command = (($Executable, $Arguments) -join " ").Trim()
            available = ($exitCode -eq 0)
            exit_code = $exitCode
            output = (($output | Out-String).Trim())
        }
    } catch {
        return [ordered]@{
            command = (($Executable, $Arguments) -join " ").Trim()
            available = $false
            exit_code = $null
            output = $_.Exception.Message
        }
    }
}

function Resolve-WasmBindgen {
    if ($env:VAE_WASM_BINDGEN -and (Test-Path -LiteralPath $env:VAE_WASM_BINDGEN)) {
        return $env:VAE_WASM_BINDGEN
    }
    $command = Get-Command wasm-bindgen.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $toolRoot = if ($env:VAE_TOOL_ROOT) { $env:VAE_TOOL_ROOT } else { Join-Path $script:repoRoot ".tools" }
    return Get-ChildItem -LiteralPath (Join-Path $toolRoot "wasm-bindgen-0.2.126") -Recurse -Filter "wasm-bindgen.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}

function Resolve-ToolExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$FallbackPath
    )
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    if (Test-Path -LiteralPath $FallbackPath) { return $FallbackPath }
    return $Name
}

function Test-IsEvidencePath {
    param([Parameter(Mandatory = $true)][string]$Pathname)
    $normalized = $Pathname.Replace("\", "/")
    return (
        $normalized.StartsWith("docs/verification/") -or
        $normalized -eq "docs/PHASE_1B_METRICS.json" -or
        $normalized -eq "docs/PHASE_1B_METRICS_ENGINE_ONLY.json" -or
        $normalized -eq "docs/REVIEW_PACKET_1B.md" -or
        $normalized -match '^docs/PHASE_1B_GATE_[^/]+$'
    )
}

function Get-StatusPath {
    param([Parameter(Mandatory = $true)][string]$Line)
    if ($Line.Length -lt 4) { return $Line }
    $pathname = $Line.Substring(3).Trim()
    if ($pathname.Contains(" -> ")) {
        $pathname = ($pathname -split " -> ")[-1]
    }
    return $pathname.Trim('"').Replace("\", "/")
}

function Set-FirstFailure {
    param(
        [Parameter(Mandatory = $true)][string]$StepId,
        [Parameter(Mandatory = $true)][string]$Message,
        [AllowNull()][object]$ExitCode,
        [string[]]$Artifacts = @()
    )
    if ($null -eq $script:manifest.first_failure) {
        $script:manifest.first_failure = [ordered]@{
            step_id = $StepId
            message = $Message
            exit_code = $ExitCode
            captured_at_utc = Get-UtcNow
            artifacts = $Artifacts
        }
    }
    $script:manifest.status = "FAIL"
    $script:manifest.gate_conclusion = "NOT PASSED"
    Write-Manifest
}

function Invoke-GateStep {
    param(
        [Parameter(Mandatory = $true)][string]$Id,
        [Parameter(Mandatory = $true)][string]$DisplayName,
        [Parameter(Mandatory = $true)][string]$Executable,
        [string[]]$Arguments = @(),
        [string[]]$Artifacts = @()
    )
    $script:activeStep = $Id
    $step = [ordered]@{
        id = $Id
        display_name = $DisplayName
        command = (($Executable, $Arguments) -join " ").Trim()
        working_directory = (Get-Location).Path
        started_at_utc = Get-UtcNow
        finished_at_utc = $null
        status = "RUNNING"
        exit_code = $null
        error = $null
        artifacts = $Artifacts
    }
    [void]$script:manifest.steps.Add($step)
    Write-Manifest
    Write-Host ""
    Write-Host "[Gate 1B] $DisplayName"
    Write-Host "command=$($step.command)"

    try {
        & $Executable @Arguments
        $exitCode = $LASTEXITCODE
        if ($null -eq $exitCode) { $exitCode = 0 }
    } catch {
        $exitCode = 1
        $step.error = $_.Exception.Message
    }
    $step.finished_at_utc = Get-UtcNow
    $step.exit_code = $exitCode
    $step.status = if ($exitCode -eq 0) { "PASS" } else { "FAIL" }
    Write-Manifest

    if ($exitCode -ne 0) {
        Set-FirstFailure -StepId $Id -Message "$DisplayName failed." -ExitCode $exitCode -Artifacts $Artifacts
        throw "$DisplayName failed with exit code $exitCode"
    }
}

function Invoke-FinalizerBestEffort {
    $node = Get-Command node.exe -ErrorAction SilentlyContinue
    if (-not $node -or -not (Test-Path -LiteralPath $script:finalizerPath)) {
        Write-Warning "Gate finalizer could not run because Node or the finalizer script is unavailable."
        return $false
    }
    & $node.Source $script:finalizerPath
    return ($LASTEXITCODE -eq 0)
}

Push-Location $repoRoot
try {
    $sourceCommit = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw "Could not resolve the source commit." }
    $sourceBranch = (& git branch --show-current).Trim()
    if ($LASTEXITCODE -ne 0) { throw "Could not resolve the current branch." }
    $startedAt = Get-UtcNow
    $runId = "phase1b-{0}-{1}-{2}" -f [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ"), $sourceCommit.Substring(0, 12), ([Guid]::NewGuid().ToString("N").Substring(0, 8))

    $chromeExecutable = if ($env:PHASE0E_CHROME) {
        $env:PHASE0E_CHROME
    } else {
        "C:\Program Files\Google\Chrome\Application\chrome.exe"
    }
    $toolRoot = if ($env:VAE_TOOL_ROOT) { $env:VAE_TOOL_ROOT } else { Join-Path $repoRoot ".tools" }
    $localCargoBin = Join-Path $toolRoot "cargo\bin"
    if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath (Join-Path $localCargoBin "cargo.exe"))) {
        $env:RUSTUP_HOME = Join-Path $toolRoot "rustup"
        $env:CARGO_HOME = Join-Path $toolRoot "cargo"
    }
    $cargoExecutable = Resolve-ToolExecutable -Name "cargo.exe" -FallbackPath (Join-Path $localCargoBin "cargo.exe")
    $rustupExecutable = Resolve-ToolExecutable -Name "rustup.exe" -FallbackPath (Join-Path $localCargoBin "rustup.exe")
    $rustcExecutable = Resolve-ToolExecutable -Name "rustc.exe" -FallbackPath (Join-Path $localCargoBin "rustc.exe")
    $wasmBindgen = Resolve-WasmBindgen
    $windowsVersion = [ordered]@{
        os_version = [System.Environment]::OSVersion.VersionString
        caption = $null
        version = $null
        build_number = $null
    }
    try {
        $windowsInfo = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
        $windowsVersion.caption = $windowsInfo.Caption
        $windowsVersion.version = $windowsInfo.Version
        $windowsVersion.build_number = $windowsInfo.BuildNumber
    } catch {
        $windowsVersion.caption = "Unavailable: $($_.Exception.Message)"
    }

    $statusLines = @(& git status --porcelain=v1 --untracked-files=all)
    if ($LASTEXITCODE -ne 0) { throw "Could not inspect the source tree before the Gate run." }
    $statusPaths = @($statusLines | Where-Object { $_ } | ForEach-Object { Get-StatusPath $_ })
    $unexpectedPaths = @($statusPaths | Where-Object { -not (Test-IsEvidencePath $_) })

    $manifest = [ordered]@{
        phase = "1B"
        record = "Windows Gate 1B orchestration run"
        gate_run_id = $runId
        tested_source_commit = $sourceCommit
        tested_branch = $sourceBranch
        started_at_utc = $startedAt
        finished_at_utc = $null
        status = "RUNNING"
        gate_conclusion = "NOT PASSED"
        environment = [ordered]@{
            windows = $windowsVersion
            chrome_executable = $chromeExecutable
            chrome_exists = (Test-Path -LiteralPath $chromeExecutable)
            node = Get-VersionRecord -Executable "node.exe" -Arguments @("--version")
            rust_toolchain = Get-VersionRecord -Executable $rustupExecutable -Arguments @("show", "active-toolchain")
            rustc = Get-VersionRecord -Executable $rustcExecutable -Arguments @("--version", "--verbose")
            cargo = Get-VersionRecord -Executable $cargoExecutable -Arguments @("--version")
            wasm_bindgen_executable = $wasmBindgen
            wasm_bindgen = if ($wasmBindgen) { Get-VersionRecord -Executable $wasmBindgen -Arguments @("--version") } else { $null }
        }
        source_tree = [ordered]@{
            initially_dirty = ($statusLines.Count -gt 0)
            initial_status = $statusLines
            initial_paths = $statusPaths
            unexpected_paths = $unexpectedPaths
            note = "Generated verification artifacts may make the tree dirty after this snapshot."
        }
        steps = (New-Object System.Collections.ArrayList)
        first_failure = $null
    }

    $env:PHASE1B_GATE_RUN_ID = $runId
    $env:PHASE1B_GATE_SOURCE_COMMIT = $sourceCommit
    $env:PHASE1B_GATE_SOURCE_BRANCH = $sourceBranch

    Write-Manifest
    Write-Host "PHASE1B_GATE_RUN_ID=$runId"
    Write-Host "PHASE1B_GATE_SOURCE_COMMIT=$sourceCommit"
    Write-Host "PHASE1B_GATE_SOURCE_BRANCH=$sourceBranch"
    Write-Host "PHASE1B_GATE_STARTED_AT_UTC=$startedAt"
    Write-Host "WINDOWS_VERSION=$($windowsVersion.caption) $($windowsVersion.version) build $($windowsVersion.build_number)"
    Write-Host "CHROME_EXECUTABLE=$chromeExecutable"
    Write-Host "NODE_VERSION=$($manifest.environment.node.output)"
    Write-Host "RUST_TOOLCHAIN=$($manifest.environment.rust_toolchain.output)"
    Write-Host "WASM_BINDGEN_VERSION=$(if ($manifest.environment.wasm_bindgen) { $manifest.environment.wasm_bindgen.output } else { 'not found' })"

    if ($statusLines.Count -gt 0) {
        Write-Warning "The source tree was dirty when Gate 1B started."
        foreach ($line in $statusLines) { Write-Host "initial_dirty=$line" }
    }
    if ($sourceBranch -ne $expectedBranch) {
        Set-FirstFailure -StepId "branch_validation" -Message "Expected branch $expectedBranch but found $sourceBranch." -ExitCode 1
        throw "Gate 1B must run on $expectedBranch."
    }
    if ($unexpectedPaths.Count -gt 0) {
        Set-FirstFailure -StepId "source_tree_validation" -Message "Unexpected initial source changes: $($unexpectedPaths -join ', ')" -ExitCode 1
        throw "Unexpected production or harness changes were present before the Gate run."
    }
    if ($env:PHASE1B_ALLOW_SOFTWARE_GPU -eq "1") {
        Set-FirstFailure -StepId "hardware_gpu_validation" -Message "PHASE1B_ALLOW_SOFTWARE_GPU=1 is diagnostic-only and cannot run Gate 1B." -ExitCode 1
        throw "Gate 1B requires a hardware GPU; unset PHASE1B_ALLOW_SOFTWARE_GPU."
    }

    $powerShell = (Get-Command powershell.exe -ErrorAction Stop).Source
    $cargoScript = Join-Path $repoRoot "tools\cargo.ps1"
    $wasmScript = Join-Path $repoRoot "tools\build-phase0e-wasm.ps1"
    $wasmSmokeScript = Join-Path $repoRoot "tools\ci-wasm-smoke.mjs"
    $gateWasmOutDir = Join-Path $repoRoot "target\phase1b-gate-wasm-pkg"
    $nodeExecutable = (Get-Command node.exe -ErrorAction Stop).Source

    Invoke-GateStep -Id "cargo_fmt" -DisplayName "cargo fmt check" -Executable $powerShell -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $cargoScript, "fmt", "--all", "--", "--check")
    Invoke-GateStep -Id "cargo_clippy" -DisplayName "cargo clippy workspace/all-targets" -Executable $powerShell -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $cargoScript, "clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
    Invoke-GateStep -Id "cargo_build" -DisplayName "cargo build workspace/all-targets" -Executable $powerShell -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $cargoScript, "build", "--workspace", "--all-targets")
    Invoke-GateStep -Id "cargo_test" -DisplayName "cargo test workspace/all-targets" -Executable $powerShell -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $cargoScript, "test", "--workspace", "--all-targets")
    Invoke-GateStep -Id "wasm_build" -DisplayName "build fresh Phase 0E WASM outside the tracked package" -Executable $powerShell -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $wasmScript, "-OutDir", $gateWasmOutDir) -Artifacts @("target/phase1b-gate-wasm-pkg/engine_host.js", "target/phase1b-gate-wasm-pkg/engine_host_bg.wasm")
    Invoke-GateStep -Id "wasm_smoke" -DisplayName "verify fresh WASM bindings, ABI, and EngineHost initialization" -Executable $nodeExecutable -Arguments @($wasmSmokeScript, "--pkg", $gateWasmOutDir) -Artifacts @("target/phase1b-gate-wasm-pkg/engine_host.js", "target/phase1b-gate-wasm-pkg/engine_host_bg.wasm")

    Set-Location $editorRoot
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source
    Invoke-GateStep -Id "npm_ci" -DisplayName "install exact editor dependencies" -Executable $npm -Arguments @("ci")
    Invoke-GateStep -Id "phase1a_browser" -DisplayName "Phase 1A browser regression" -Executable $npm -Arguments @("run", "test:browser:phase1a") -Artifacts @("docs/verification/PHASE_1A_BROWSER_PROOF.json", "docs/verification/PHASE_1A_PIXEL_READBACK.json")
    Invoke-GateStep -Id "phase1b_browser" -DisplayName "Phase 1B actual browser/WebGPU proof" -Executable $npm -Arguments @("run", "test:browser:phase1b") -Artifacts @("docs/verification/PHASE_1B_BROWSER_PROOF.json", "docs/verification/PHASE_1B_PIXEL_READBACK.json", "docs/PHASE_1B_METRICS.json")
    Invoke-GateStep -Id "direct_wasm" -DisplayName "Phase 1B direct WASM proof" -Executable $npm -Arguments @("run", "test:direct-wasm:phase1b") -Artifacts @("docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json")
    Invoke-GateStep -Id "engine_benchmark" -DisplayName "Phase 1B engine multi-drag benchmark" -Executable $npm -Arguments @("run", "bench:multi-drag:phase1b") -Artifacts @("docs/PHASE_1B_METRICS_ENGINE_ONLY.json")
    Invoke-GateStep -Id "editor_build" -DisplayName "production editor build" -Executable $npm -Arguments @("run", "build")

    Invoke-GateStep -Id "gate_finalize" -DisplayName "automatic Gate 1B finalization" -Executable $npm -Arguments @("run", "finalize:gate:phase1b") -Artifacts @("docs/verification/PHASE_1B_GATE_STATUS.json")
    Invoke-GateStep -Id "default_tests" -DisplayName "default editor test suite and Gate guard" -Executable $npm -Arguments @("test")

    $manifest.status = "PASS"
    $manifest.gate_conclusion = "PASSED"
    $manifest.finished_at_utc = Get-UtcNow
    Write-Manifest
    if (-not (Invoke-FinalizerBestEffort)) {
        Set-FirstFailure -StepId "final_status_persist" -Message "Final Gate status could not be persisted after the guard passed." -ExitCode 1 -Artifacts @("docs/verification/PHASE_1B_GATE_STATUS.json")
        throw "Final Gate status persistence failed."
    }

    Write-Host ""
    Write-Host "Gate 1B PASSED"
    Write-Host "gate_run_id=$runId"
    Write-Host "tested_source_commit=$sourceCommit"
    exit 0
} catch {
    if ($null -ne $manifest) {
        if ($null -eq $manifest.first_failure) {
            Set-FirstFailure -StepId $activeStep -Message $_.Exception.Message -ExitCode 1
        }
        $manifest.status = "FAIL"
        $manifest.gate_conclusion = "NOT PASSED"
        $manifest.finished_at_utc = Get-UtcNow
        Write-Manifest
        [void](Invoke-FinalizerBestEffort)

        [Console]::Error.WriteLine("Gate 1B NOT PASSED: $($_.Exception.Message)")
        Write-Host "first_failure=$($manifest.first_failure.step_id)"
        Write-Host "evidence=$($manifest.first_failure.artifacts -join ',')"
        Write-Host "chrome=$($manifest.environment.chrome_executable)"
        Write-Host "source_commit=$($manifest.tested_source_commit)"
        Write-Host "gate_run_id=$($manifest.gate_run_id)"
    } else {
        [Console]::Error.WriteLine("Gate 1B could not start: $($_.Exception.Message)")
    }
    exit 1
} finally {
    Pop-Location
}
