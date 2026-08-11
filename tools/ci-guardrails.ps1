$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$tracked = @(git -C $repoRoot ls-files)
if ($LASTEXITCODE -ne 0) { throw "git ls-files failed" }

$forbidden = [Collections.Generic.List[string]]::new()
foreach ($path in $tracked) {
    $normalized = $path.Replace("\", "/")
    if ($normalized -match "(^|/)(target|node_modules|dist|coverage|\.tools|\.cache|\.vite|\.vitest|\.npm)(/|$)" -or
        $normalized -match "\.tsbuildinfo$" -or
        $normalized -match "\.(zip|zip\.sha256|log|tmp|temp)$" -or
        $normalized -match "(^|/)\.env($|\.)" -and $normalized -notmatch "\.env\.example$" -or
        $normalized -match "\.(pem|pfx|key)$") {
        $forbidden.Add($path)
    }
}
if ($forbidden.Count -ne 0) { throw "Forbidden tracked artifacts: $($forbidden -join ', ')" }

$lfsPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
@(git -C $repoRoot lfs ls-files --name-only) | ForEach-Object { [void]$lfsPaths.Add($_.Replace("\", "/")) }
$largeViolations = [Collections.Generic.List[string]]::new()
foreach ($path in $tracked) {
    $full = Join-Path $repoRoot $path
    if ((Test-Path -LiteralPath $full -PathType Leaf) -and (Get-Item -LiteralPath $full).Length -gt 20MB -and -not $lfsPaths.Contains($path.Replace("\", "/"))) {
        $largeViolations.Add($path)
    }
}
if ($largeViolations.Count -ne 0) { throw "Tracked files above 20 MiB must use Git LFS: $($largeViolations -join ', ')" }

$secretPattern = "AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36,}|-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"
$secretMatches = @(git -C $repoRoot grep -I -n -E -- $secretPattern -- . 2>$null)
if ($LASTEXITCODE -notin @(0, 1)) { throw "git grep secret scan failed" }
if ($secretMatches.Count -ne 0) { throw "Potential tracked secret material: $($secretMatches -join '; ')" }

[pscustomobject]@{
    tracked_files = $tracked.Count
    forbidden_artifacts = $forbidden.Count
    lfs_files = $lfsPaths.Count
    untracked_large_files = $largeViolations.Count
    secret_matches = $secretMatches.Count
    all_passed = $true
} | ConvertTo-Json -Depth 4
exit 0
