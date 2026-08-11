[CmdletBinding()]
param(
    [string]$BaseRef,
    [string]$HeadRef = "HEAD",
    [string[]]$ChangedPath,
    [string]$GitHubOutputPath = $env:GITHUB_OUTPUT,
    [switch]$NoGitHubOutput
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot

if ($null -eq $ChangedPath -or $ChangedPath.Count -eq 0) {
    if ([string]::IsNullOrWhiteSpace($BaseRef)) {
        throw "BaseRef is required when ChangedPath is not supplied"
    }
    $ChangedPath = @(git -C $repoRoot diff --name-only --diff-filter=ACMRD "$BaseRef...$HeadRef" --)
    if ($LASTEXITCODE -ne 0) { throw "git diff failed for $BaseRef...$HeadRef" }
}

$paths = @(
    $ChangedPath |
        ForEach-Object {
            $normalized = $_.Replace("\", "/")
            if ($normalized.StartsWith("./", [StringComparison]::Ordinal)) { $normalized.Substring(2) } else { $normalized }
        } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        Sort-Object -Unique
)
if ($paths.Count -eq 0) { throw "Change scope is empty; refusing to skip required checks" }

$rust = $false
$wasm = $false
$editorWeb = $false
$previewWeb = $false
$renderOrGpu = $false
$ciOrUnknown = $false
$allDocs = $true
$reasons = [Collections.Generic.List[string]]::new()
$productPaths = [Collections.Generic.List[string]]::new()
$testPaths = [Collections.Generic.List[string]]::new()

foreach ($path in $paths) {
    $isDoc = $path -match "^docs/" -or $path -match "(^|/)(README|CONTRIBUTING|SECURITY)\.md$"
    $isTest = $path -match "(^|/)(tests?|__tests__)(/|$)" -or $path -match "(_tests?\.rs|\.test\.[cm]?[jt]sx?|\.spec\.[cm]?[jt]sx?)$"
    $isCi = $path -match "^\.github/" -or
        $path -match "^tools/(ci-|verify-approved-phase0e-r3|validate-hardware-evidence|validate-pr-hardware-evidence)" -or
        $path -eq "AGENTS.md"
    $isCargo = $path -in @("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml") -or
        $path -match "^crates/.+/Cargo\.toml$"
    $isRust = $isCargo -or $path -match "^crates/.+\.rs$" -or $path -match "^tools/cargo\.ps1$"
    $isWasm = $isCargo -or $path -match "^crates/" -or
        $path -match "^tools/build-phase0[a-z0-9-]*-wasm\.ps1$" -or
        $path -match "^shared/(render_binary_schema\.json|render_contract\.wgsl)$" -or
        $path -match "(^|/)(protocol|worker|engine_host)"
    $isEditor = $path -match "^web/editor/"
    $isPreview = $path -match "^web/phase0d-preview/"
    $isRender = $path -match "\.wgsl$" -or
        $path -match "^crates/renderer_wgpu/" -or
        $path -eq "shared/render_binary_schema.json" -or
        $path -match "^web/(editor|phase0d-preview)/.+(renderer|render_contract|webgpu|gpu)"

    if ($isDoc) {
        $isCargo = $false
        $isRust = $false
        $isWasm = $false
        $isEditor = $false
        $isPreview = $false
        $isRender = $false
    }

    if (-not $isDoc) { $allDocs = $false }
    if ($isTest) { $testPaths.Add($path) }
    if (-not $isDoc -and -not $isTest -and -not $isCi -and $path -notmatch "^tools/") {
        $productPaths.Add($path)
    }

    if ($isCi) {
        $ciOrUnknown = $true
        $reasons.Add("full:ci-or-governance:$path")
    }
    if ($isRust) {
        $rust = $true
        $reasons.Add("rust:$path")
    }
    if ($isWasm) {
        $wasm = $true
        $reasons.Add("wasm:$path")
    }
    if ($isEditor) {
        $editorWeb = $true
        $reasons.Add("editor-web:$path")
    }
    if ($isPreview) {
        $previewWeb = $true
        $reasons.Add("preview-web:$path")
    }
    if ($isRender) {
        $renderOrGpu = $true
        $rust = $true
        $wasm = $true
        $editorWeb = $true
        $previewWeb = $true
        $reasons.Add("render-or-gpu:full-software-plus-hardware-evidence:$path")
    }

    $isKnown = $isDoc -or $isCi -or $isRust -or $isWasm -or $isEditor -or $isPreview -or $isRender
    if (-not $isKnown) {
        $ciOrUnknown = $true
        $allDocs = $false
        $reasons.Add("full:unknown-fail-closed:$path")
    }
}

if ($ciOrUnknown) {
    $rust = $true
    $wasm = $true
    $editorWeb = $true
    $previewWeb = $true
}

$docsOnly = $allDocs -and -not $ciOrUnknown
$result = [ordered]@{
    docs_only = $docsOnly
    rust = $rust
    wasm = $wasm
    editor_web = $editorWeb
    preview_web = $previewWeb
    render_or_gpu = $renderOrGpu
    ci_or_unknown = $ciOrUnknown
    changed_paths = $paths
    product_changes = @($productPaths | Sort-Object -Unique)
    test_changes = @($testPaths | Sort-Object -Unique)
    reasons = @($reasons | Sort-Object -Unique)
}

if (-not $NoGitHubOutput -and -not [string]::IsNullOrWhiteSpace($GitHubOutputPath)) {
    foreach ($name in @("docs_only", "rust", "wasm", "editor_web", "preview_web", "render_or_gpu", "ci_or_unknown")) {
        $value = [bool]$result[$name]
        Add-Content -LiteralPath $GitHubOutputPath -Value "$name=$($value.ToString().ToLowerInvariant())" -Encoding utf8
    }
    Add-Content -LiteralPath $GitHubOutputPath -Value "changed_count=$($paths.Count)" -Encoding utf8
    Add-Content -LiteralPath $GitHubOutputPath -Value "product_change_count=$($result.product_changes.Count)" -Encoding utf8
    Add-Content -LiteralPath $GitHubOutputPath -Value "test_change_count=$($result.test_changes.Count)" -Encoding utf8
}

$result | ConvertTo-Json -Depth 6
