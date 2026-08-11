$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$guardrails = Join-Path $PSScriptRoot "ci-guardrails.ps1"
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("editorm-guardrails-" + [Guid]::NewGuid().ToString("N"))
$safeMarker = "SYNTHETIC_CI_SECRET_MARKER_DO_NOT_USE"
$currentPowerShell = (Get-Process -Id $PID).Path

try {
    [void](New-Item -ItemType Directory -Path $testRoot)
    & git -C $testRoot init --quiet
    if ($LASTEXITCODE -ne 0) { throw "Synthetic repository initialization failed" }
    & git -C $testRoot config user.name "EditorM CI"
    & git -C $testRoot config user.email "ci@example.invalid"
    [IO.File]::WriteAllText((Join-Path $testRoot "safe-marker.txt"), $safeMarker, [Text.UTF8Encoding]::new($false))
    & git -C $testRoot add -- safe-marker.txt
    & git -C $testRoot commit --quiet -m "synthetic fixture"
    if ($LASTEXITCODE -ne 0) { throw "Synthetic repository commit failed" }
    $baseline = (& git -C $testRoot rev-parse HEAD).Trim()

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $output = (& $currentPowerShell -NoProfile -File $guardrails -RepositoryRoot $testRoot -BaselineSha $baseline -AdditionalSecretPattern $safeMarker -SkipApprovedLfsObjectCheck 2>&1 | Out-String)
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousPreference
    if ($exitCode -eq 0) { throw "Synthetic secret fixture was not rejected" }
    if ($output.IndexOf($safeMarker, [StringComparison]::Ordinal) -ge 0) { throw "Secret value leaked into guardrail output" }
    if ($output.IndexOf("safe-marker.txt", [StringComparison]::Ordinal) -lt 0) { throw "Guardrail output did not identify the affected file path" }

    [pscustomobject]@{
        synthetic_fixture = "safe non-credential marker"
        rejected = $true
        value_disclosed = $false
        file_path_reported = $true
        all_passed = $true
    } | ConvertTo-Json
}
finally {
    $resolvedTemp = [IO.Path]::GetFullPath($testRoot)
    $resolvedSystemTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ($resolvedTemp.StartsWith($resolvedSystemTemp, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedTemp) -like "editorm-guardrails-*") {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

exit 0
