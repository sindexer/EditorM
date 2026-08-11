[CmdletBinding()]
param(
    [string]$RepositoryRoot,
    [string]$BaselineSha = "39085167a1b9d2ce1ba78060b3fee4d9327aaf27",
    [string]$HeadRef = "HEAD",
    [string]$AdditionalSecretPattern,
    [switch]$SkipApprovedLfsObjectCheck
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = Split-Path -Parent $PSScriptRoot
}

$repoRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$tracked = @(git -C $repoRoot ls-files)
if ($LASTEXITCODE -ne 0) { throw "git ls-files failed" }

& git -C $repoRoot cat-file -e "$BaselineSha^{commit}" 2>$null
if ($LASTEXITCODE -ne 0) { throw "Approved baseline commit is unavailable: $BaselineSha" }
& git -C $repoRoot merge-base --is-ancestor $BaselineSha $HeadRef
if ($LASTEXITCODE -ne 0) { throw "Approved baseline is not an ancestor of $HeadRef" }

$baselinePaths = @(git -C $repoRoot ls-tree -r --name-only $BaselineSha --)
if ($LASTEXITCODE -ne 0) { throw "Unable to enumerate approved baseline paths" }
$protected = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach ($path in $baselinePaths) {
    $normalized = $path.Replace("\", "/")
    $isProtected = $normalized -eq "CHECKSUMS.sha256" -or
        $normalized -match "^docs/PHASE_0" -or
        $normalized -match "^docs/REVIEW_PACKET_0" -or
        $normalized -match "^docs/verification/PHASE_0" -or
        $normalized -match "^visual_authoring_engine_codex_package/"
    if ($normalized -match "^docs/adr/ADR-(\d{3})-") {
        $adrNumber = [int]$Matches[1]
        if ($adrNumber -ge 1 -and $adrNumber -le 41) { $isProtected = $true }
    }
    if ($isProtected) { [void]$protected.Add($normalized) }
}

$baselineDiff = @(git -C $repoRoot diff --name-only $BaselineSha $HeadRef --)
if ($LASTEXITCODE -ne 0) { throw "Unable to compare protected evidence with the approved baseline" }
$protectedChanges = @(
    $baselineDiff |
        ForEach-Object { $_.Replace("\", "/") } |
        Where-Object { $protected.Contains($_) }
)
if ($protectedChanges.Count -ne 0) {
    throw "Protected Phase 0 evidence changed or was deleted ($($protectedChanges.Count) file(s)): $($protectedChanges -join ', ')"
}

$forbidden = [Collections.Generic.List[string]]::new()
foreach ($path in $tracked) {
    $normalized = $path.Replace("\", "/")
    if ($normalized -match "(^|/)(target|node_modules|dist|coverage|\.tools|\.cache|\.vite|\.vitest|\.npm)(/|$)" -or
        $normalized -match "\.tsbuildinfo$" -or
        $normalized -match "\.(zip|zip\.sha256|log|tmp|temp)$" -or
        ($normalized -match "(^|/)\.env($|\.)" -and $normalized -notmatch "\.env\.example$") -or
        $normalized -match "(^|/)(credentials?|secrets?)(\.|/|$)" -or
        $normalized -match "\.(pem|pfx|key)$") {
        $forbidden.Add($path)
    }
}
if ($forbidden.Count -ne 0) { throw "Forbidden tracked artifacts ($($forbidden.Count) file(s)): $($forbidden -join ', ')" }

$lfsPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
@(git -C $repoRoot lfs ls-files --name-only) | ForEach-Object { [void]$lfsPaths.Add($_.Replace("\", "/")) }
if ($LASTEXITCODE -ne 0) { throw "git lfs ls-files failed" }

$lfsExtensionViolations = @(
    $tracked |
        Where-Object { $_ -match "\.(fig|psd)$" -and -not $lfsPaths.Contains($_.Replace("\", "/")) }
)
if ($lfsExtensionViolations.Count -ne 0) {
    throw "Approved binary originals must use Git LFS ($($lfsExtensionViolations.Count) file(s)): $($lfsExtensionViolations -join ', ')"
}

if (-not $SkipApprovedLfsObjectCheck) {
    $expectedLfsPath = "visual_authoring_engine_codex_package/reference/wanted-design-system/Wanted Design System (Community).fig"
    $expectedLfsOid = "d6f87f906ef2cbf211cae4b7bfe92429c064a4d783529ff14386b0a809fe7887"
    $lfsListing = @(git -C $repoRoot lfs ls-files -l)
    if ($LASTEXITCODE -ne 0 -or -not ($lfsListing -match "^$expectedLfsOid . $([regex]::Escape($expectedLfsPath))$")) {
        throw "Approved Figma source is not tracked by the expected Git LFS object"
    }
}

$largeViolations = [Collections.Generic.List[string]]::new()
foreach ($path in $tracked) {
    $full = Join-Path $repoRoot $path
    if ((Test-Path -LiteralPath $full -PathType Leaf) -and
        (Get-Item -LiteralPath $full).Length -gt 20MB -and
        -not $lfsPaths.Contains($path.Replace("\", "/"))) {
        $largeViolations.Add($path)
    }
}
if ($largeViolations.Count -ne 0) {
    throw "Tracked files above 20 MiB must use Git LFS ($($largeViolations.Count) file(s)): $($largeViolations -join ', ')"
}

$secretPatterns = [Collections.Generic.List[string]]::new()
$secretPatterns.Add("AKIA[0-9A-Z]{16}")
$secretPatterns.Add("gh[pousr]_[A-Za-z0-9]{36,}")
$secretPatterns.Add("-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----")
$secretPatterns.Add('[Qq][Ww][Ee][Nn]_([Aa][Pp][Ii]_[Kk][Ee][Yy]|[Tt][Oo][Kk][Ee][Nn]|[Pp][Aa][Ss][Ss][Ww][Oo][Rr][Dd])[[:space:]]*[:=][[:space:]]*[[:punct:]]?[A-Za-z0-9_./+=-]{20,}')
if (-not [string]::IsNullOrWhiteSpace($AdditionalSecretPattern)) {
    $secretPatterns.Add($AdditionalSecretPattern)
}
$grepArguments = [Collections.Generic.List[string]]::new()
foreach ($argument in @("-C", $repoRoot, "grep", "-I", "-l", "-E")) { $grepArguments.Add($argument) }
foreach ($pattern in $secretPatterns) {
    $grepArguments.Add("-e")
    $grepArguments.Add($pattern)
}
$grepArguments.Add("--")
$grepArguments.Add(".")
$secretMatches = @(& git @grepArguments 2>$null)
$grepExit = $LASTEXITCODE
if ($grepExit -notin @(0, 1)) { throw "Tracked secret scan failed" }
$secretPaths = @($secretMatches | Sort-Object -Unique)
if ($secretPaths.Count -ne 0) {
    throw "Potential tracked secret material found in $($secretPaths.Count) file(s): $($secretPaths -join ', ')"
}

[pscustomobject]@{
    baseline_sha = $BaselineSha
    baseline_is_ancestor = $true
    protected_phase0_files = $protected.Count
    protected_phase0_changes = $protectedChanges.Count
    tracked_files = $tracked.Count
    forbidden_artifacts = $forbidden.Count
    lfs_files = $lfsPaths.Count
    non_lfs_large_files = $largeViolations.Count
    secret_patterns = $secretPatterns.Count
    secret_files = $secretPaths.Count
    all_passed = $true
} | ConvertTo-Json -Depth 4
exit 0
