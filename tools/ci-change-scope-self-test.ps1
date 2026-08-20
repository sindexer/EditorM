$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$classifier = Join-Path $PSScriptRoot "ci-change-scope.ps1"
$cases = @(
    @{ name = "docs-only"; paths = @("docs/worker-protocol.md"); expect = @{ docs_only = $true; rust = $false; wasm = $false; editor_web = $false; preview_web = $false; render_or_gpu = $false; ci_or_unknown = $false } },
    @{ name = "rust"; paths = @("crates/document/src/lib.rs"); expect = @{ docs_only = $false; rust = $true; wasm = $true; editor_web = $false; preview_web = $false; render_or_gpu = $false; ci_or_unknown = $false } },
    @{ name = "wasm-bridge"; paths = @("crates/wasm_bridge/src/lib.rs"); expect = @{ rust = $true; wasm = $true; ci_or_unknown = $false } },
    @{ name = "wgsl"; paths = @("shared/render_contract.wgsl"); expect = @{ rust = $true; wasm = $true; editor_web = $true; preview_web = $true; render_or_gpu = $true; ci_or_unknown = $false } },
    @{ name = "editor-react"; paths = @("web/editor/src/App.tsx"); expect = @{ rust = $false; wasm = $false; editor_web = $true; preview_web = $false; render_or_gpu = $false; ci_or_unknown = $false } },
    @{ name = "editor-engine"; paths = @("web/editor/src/engine.ts"); expect = @{ rust = $true; wasm = $true; editor_web = $true; preview_web = $true; render_or_gpu = $true; ci_or_unknown = $false } },
    @{ name = "preview"; paths = @("web/phase0d-preview/src/app.js"); expect = @{ rust = $false; wasm = $false; editor_web = $false; preview_web = $true; render_or_gpu = $false; ci_or_unknown = $false } },
    @{ name = "workflow"; paths = @(".github/workflows/pr-fast.yml"); expected_path = ".github/workflows/pr-fast.yml"; expected_reason = "full:ci-or-governance:.github/workflows/pr-fast.yml"; expect = @{ rust = $true; wasm = $true; editor_web = $true; preview_web = $true; ci_or_unknown = $true } },
    @{ name = "unknown-product"; paths = @("future_product/engine.bin"); expect = @{ rust = $true; wasm = $true; editor_web = $true; preview_web = $true; ci_or_unknown = $true } }
)

$passed = 0
foreach ($case in $cases) {
    $actual = (& $classifier -ChangedPath $case.paths -NoGitHubOutput | ConvertFrom-Json)
    foreach ($entry in $case.expect.GetEnumerator()) {
        if ([bool]$actual.($entry.Key) -ne [bool]$entry.Value) {
            throw "Case '$($case.name)' expected $($entry.Key)=$($entry.Value) but received $($actual.($entry.Key))"
        }
    }
    if ($case.ContainsKey("expected_path") -and $actual.changed_paths -notcontains $case.expected_path) {
        throw "Case '$($case.name)' did not preserve the normalized path"
    }
    if ($case.ContainsKey("expected_reason") -and $actual.reasons -notcontains $case.expected_reason) {
        throw "Case '$($case.name)' did not emit the expected classification reason"
    }
    $passed++
}

[pscustomobject]@{
    cases = $cases.Count
    passed = $passed
    all_passed = $true
} | ConvertTo-Json
