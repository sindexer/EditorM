$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$checksumPath = Join-Path $repoRoot "CHECKSUMS.sha256"
$expectedChecksumFileHash = "54db9e92690a4f8f06b8a499828a51f1127b883cbbd639fdd577e0a3c57b6b55"
$expectedGovernanceIgnoreHash = "f9f02addd19d5e03637716c139dead1f61b314b6a3133de9bbccd5a200af266e"
$allowedOperationalDifferences = @(".gitignore")

$checksumFileHash = (Get-FileHash -LiteralPath $checksumPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($checksumFileHash -ne $expectedChecksumFileHash) {
    throw "CHECKSUMS.sha256 changed: $checksumFileHash"
}

$lines = @(Get-Content -LiteralPath $checksumPath | Where-Object { $_.Trim().Length -gt 0 })
$passed = 0
$allowed = 0
$failures = [Collections.Generic.List[string]]::new()
foreach ($line in $lines) {
    if ($line -notmatch "^([0-9a-fA-F]{64})\s{2}(.+)$") { throw "Invalid checksum line: $line" }
    $expected = $Matches[1].ToLowerInvariant()
    $relative = $Matches[2]
    $path = Join-Path $repoRoot $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $failures.Add("missing:$relative")
        continue
    }
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -eq $expected) {
        $passed++
    } elseif ($relative -in $allowedOperationalDifferences) {
        $allowed++
        if ($relative -eq ".gitignore" -and $actual -ne $expectedGovernanceIgnoreHash) {
            $failures.Add("unexpected-operational-content:$relative")
        }
    } else {
        $failures.Add("mismatch:$relative")
    }
}
if ($lines.Count -ne 297 -or $passed -ne 296 -or $allowed -ne 1 -or $failures.Count -ne 0) {
    throw "Approved baseline verification failed: entries=$($lines.Count), passed=$passed, allowed=$allowed, failures=$($failures -join ',')"
}

$lfsPath = "visual_authoring_engine_codex_package/reference/wanted-design-system/Wanted Design System (Community).fig"
$lfsListing = @(git -C $repoRoot lfs ls-files -l)
if ($LASTEXITCODE -ne 0 -or -not ($lfsListing -match "^d6f87f906ef2cbf211cae4b7bfe92429c064a4d783529ff14386b0a809fe7887 . $([regex]::Escape($lfsPath))$")) {
    throw "Approved Figma source is not tracked by the expected Git LFS object"
}
$figPath = Join-Path $repoRoot $lfsPath
$figHash = (Get-FileHash -LiteralPath $figPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($figHash -ne "d6f87f906ef2cbf211cae4b7bfe92429c064a4d783529ff14386b0a809fe7887") {
    throw "Approved Figma worktree content is not present; ensure Git LFS objects were pulled"
}

[pscustomobject]@{
    checksum_file_sha256 = $checksumFileHash
    approved_entries = $lines.Count
    byte_identical = "$passed/296"
    intended_operational_differences = $allowed
    lfs_object_sha256 = $figHash
    all_passed = $true
} | ConvertTo-Json -Depth 4
