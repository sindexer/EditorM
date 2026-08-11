param(
    [switch]$PrepareOnly,
    [string]$BaselineZip = "C:/Users/thdwl/Documents/Codex/visual_authoring_engine_phase0d_r1_p1_review_2026-08-10.zip",
    [string]$OutputZip = "C:/Users/thdwl/Documents/Codex/visual_authoring_engine_phase0e_review_2026-08-10.zip"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$repoRoot = (Resolve-Path (Split-Path -Parent $PSScriptRoot)).Path
$expectedBaselineHash = "09f91d68eead8e3f0d7c6317f1bd5e7f03e4d85eaa2636f6f0ef21c553ffbde7"
$manifestRelative = "docs/PHASE_0E_CHANGE_MANIFEST.json"
$checksumsRelative = "CHECKSUMS.sha256"
$manifestFull = Join-Path $repoRoot $manifestRelative
$checksumsFull = Join-Path $repoRoot $checksumsRelative

$forbiddenSegments = @(
    ".tools", "target", "node_modules", ".git", ".idea", ".vscode",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".cache"
)
$obsoletePhase0eScreenshots = @(
    "docs/verification/phase0e-default.png",
    "docs/verification/phase0e-selection.png",
    "docs/verification/phase0e-nested-group.png"
)
$priorEvidence = @(
    "docs/PHASE_0C_METRICS.json",
    "docs/verification/PHASE_0C_VERIFICATION.txt",
    "docs/PHASE_0D_METRICS.json",
    "docs/verification/PHASE_0D_BROWSER_PROOF.json",
    "docs/verification/PHASE_0D_VERIFICATION.txt",
    "docs/verification/phase0d-preview.png",
    "docs/PHASE_0D_R1_METRICS.json",
    "docs/verification/PHASE_0D_R1_VERIFICATION.txt",
    "docs/verification/PHASE_0D_R1_BROWSER_PROOF.json",
    "docs/verification/PHASE_0D_R1_PIXEL_READBACK.json",
    "docs/verification/phase0d-r1-preview.png",
    "docs/REVIEW_PACKET_0D_R1.md",
    "docs/PHASE_0D_R1_P1_EVIDENCE_PROVENANCE.md"
)

function Get-Sha256File([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-Sha256Stream([IO.Stream]$Stream) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return (($sha.ComputeHash($Stream) | ForEach-Object { $_.ToString("x2") }) -join "")
    } finally {
        $sha.Dispose()
    }
}

function Get-RelativePath([string]$FullName) {
    return $FullName.Substring($repoRoot.Length + 1).Replace("\", "/")
}

function Test-PackageIncluded([string]$Relative) {
    $normalized = $Relative.Replace("\", "/")
    $segments = $normalized.Split("/")
    foreach ($segment in $segments) {
        if ($forbiddenSegments -contains $segment) { return $false }
    }
    $name = $segments[-1]
    if ($name -like "phase0e_*codemod*.mjs") { return $false }
    if ($obsoletePhase0eScreenshots -contains $normalized) { return $false }
    if ($name -like "*.tsbuildinfo" -or $name -like "*.tmp" -or $name -like "*.log" -or $name -like "*.swp") { return $false }
    if ($name -eq ".DS_Store" -or $name -eq "Thumbs.db") { return $false }
    if ($name -eq ".env" -or $name -like ".env.*" -or $name -like "*.pem" -or $name -like "*.pfx") { return $false }
    if ($name -like "*.zip" -or $name -like "*.zip.sha256") { return $false }
    return $true
}

function Get-CurrentFiles {
    return @(Get-ChildItem -LiteralPath $repoRoot -Recurse -Force -File | Where-Object {
        Test-PackageIncluded (Get-RelativePath $_.FullName)
    })
}

if (-not (Test-Path -LiteralPath $BaselineZip -PathType Leaf)) { throw "Baseline ZIP not found: $BaselineZip" }
$baselineHash = Get-Sha256File $BaselineZip
if ($baselineHash -ne $expectedBaselineHash) { throw "Baseline ZIP SHA-256 mismatch: $baselineHash" }

$baselineArchive = [IO.Compression.ZipFile]::OpenRead($BaselineZip)
try {
    $baselineHashes = @{}
    foreach ($entry in $baselineArchive.Entries) {
        if ([string]::IsNullOrEmpty($entry.Name)) { continue }
        $entryPath = $entry.FullName.Replace("\", "/")
        if (-not $entryPath.StartsWith("WebEditor/", [StringComparison]::Ordinal)) {
            throw "Baseline entry is outside WebEditor root: $entryPath"
        }
        $relative = $entryPath.Substring("WebEditor/".Length)
        $stream = $entry.Open()
        try { $baselineHashes[$relative] = Get-Sha256Stream $stream } finally { $stream.Dispose() }
    }

    $authoritativeFiles = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot "visual_authoring_engine_codex_package") -Recurse -File)
    if ($authoritativeFiles.Count -ne 19) { throw "Expected 19 authoritative physical files, found $($authoritativeFiles.Count)" }
    $authoritativeChanged = [Collections.Generic.List[string]]::new()
    foreach ($file in $authoritativeFiles) {
        $relative = Get-RelativePath $file.FullName
        if (-not $baselineHashes.ContainsKey($relative) -or $baselineHashes[$relative] -ne (Get-Sha256File $file.FullName)) {
            $authoritativeChanged.Add($relative)
        }
    }
    if ($authoritativeChanged.Count -ne 0) { throw "Authoritative files changed: $($authoritativeChanged -join ', ')" }

    $sumFile = Join-Path $repoRoot "visual_authoring_engine_codex_package/SHA256SUMS.txt"
    $authoritativeSumPassed = 0
    foreach ($line in Get-Content -Encoding UTF8 -LiteralPath $sumFile) {
        if ($line -notmatch "^([0-9a-f]{64})  (.+)$") { throw "Invalid authoritative checksum line: $line" }
        $expected = $Matches[1]
        $path = Join-Path (Split-Path -Parent $sumFile) $Matches[2]
        if ((Get-Sha256File $path) -ne $expected) { throw "Authoritative checksum mismatch: $($Matches[2])" }
        $authoritativeSumPassed += 1
    }
    if ($authoritativeSumPassed -ne 18) { throw "Expected 18 authoritative checksum entries, found $authoritativeSumPassed" }

    $priorEvidenceChanged = [Collections.Generic.List[string]]::new()
    foreach ($relative in $priorEvidence) {
        $full = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "Prior evidence missing: $relative" }
        if (-not $baselineHashes.ContainsKey($relative) -or $baselineHashes[$relative] -ne (Get-Sha256File $full)) {
            $priorEvidenceChanged.Add($relative)
        }
    }
    if ($priorEvidenceChanged.Count -ne 0) { throw "Prior evidence changed: $($priorEvidenceChanged -join ', ')" }

    $currentFilesBeforeManifest = Get-CurrentFiles
    $currentHashes = @{}
    foreach ($file in $currentFilesBeforeManifest) {
        $relative = Get-RelativePath $file.FullName
        if ($relative -eq $checksumsRelative) { continue }
        $currentHashes[$relative] = Get-Sha256File $file.FullName
    }
    if (-not $currentHashes.ContainsKey($manifestRelative)) { $currentHashes[$manifestRelative] = "generated" }

    $added = @($currentHashes.Keys | Where-Object { -not $baselineHashes.ContainsKey($_) } | Sort-Object)
    $modified = @($currentHashes.Keys | Where-Object {
        $baselineHashes.ContainsKey($_) -and $currentHashes[$_] -ne $baselineHashes[$_]
    } | Sort-Object)
    if ($baselineHashes.ContainsKey($checksumsRelative) -and $modified -notcontains $checksumsRelative) {
        $modified = @($modified + $checksumsRelative | Sort-Object)
    }
    $presentForDeletion = @{} + $currentHashes
    $presentForDeletion[$checksumsRelative] = "generated"
    $deleted = @($baselineHashes.Keys | Where-Object { -not $presentForDeletion.ContainsKey($_) } | Sort-Object)

    $manifest = [ordered]@{
        phase = "0E"
        generated_at_utc = [DateTime]::UtcNow.ToString("o")
        baseline = [ordered]@{
            file = [IO.Path]::GetFileName($BaselineZip)
            sha256 = $baselineHash
        }
        counts = [ordered]@{ added = $added.Count; modified = $modified.Count; deleted = $deleted.Count }
        added = $added
        modified = $modified
        deleted = $deleted
        prior_stage_evidence = [ordered]@{ files_checked = $priorEvidence.Count; changed = 0 }
        authoritative = [ordered]@{ physical_files = 19; byte_identical = 19; checksum_entries_passed = 18; checksum_entries_total = 18 }
    }
    [IO.File]::WriteAllText($manifestFull, (($manifest | ConvertTo-Json -Depth 8) + "`n"), [Text.UTF8Encoding]::new($false))

    $files = Get-CurrentFiles
    $checksumLines = [Collections.Generic.List[string]]::new()
    foreach ($file in ($files | Sort-Object { Get-RelativePath $_.FullName })) {
        $relative = Get-RelativePath $file.FullName
        if ($relative -eq $checksumsRelative) { continue }
        $checksumLines.Add("$(Get-Sha256File $file.FullName)  $relative")
    }
    [IO.File]::WriteAllLines($checksumsFull, $checksumLines, [Text.UTF8Encoding]::new($false))

    $files = Get-CurrentFiles
    $checksumVerified = 0
    foreach ($line in Get-Content -Encoding UTF8 -LiteralPath $checksumsFull) {
        if ($line -notmatch "^([0-9a-f]{64})  (.+)$") { throw "Invalid CHECKSUMS line: $line" }
        $full = Join-Path $repoRoot $Matches[2]
        if ((Get-Sha256File $full) -ne $Matches[1]) { throw "CHECKSUMS mismatch: $($Matches[2])" }
        $checksumVerified += 1
    }
    if ($checksumVerified -ne ($files.Count - 1)) { throw "CHECKSUMS count mismatch" }

    $forbiddenCurrent = @(Get-ChildItem -LiteralPath $repoRoot -Recurse -Force -File | Where-Object {
        $relative = Get-RelativePath $_.FullName
        (Test-PackageIncluded $relative) -and (
            $relative -match "(^|/)(target|node_modules|\.tools|\.git|\.idea|\.vscode)(/|$)" -or
            $relative -match "(?i)(credential|secret|\.env($|\.)|\.pem$|\.pfx$|\.zip$)"
        )
    })
    if ($forbiddenCurrent.Count -ne 0) { throw "Forbidden included files detected before packaging" }

    Write-Output "baseline_zip_sha256=$baselineHash"
    Write-Output "manifest_added=$($added.Count)"
    Write-Output "manifest_modified=$($modified.Count)"
    Write-Output "manifest_deleted=$($deleted.Count)"
    Write-Output "prior_evidence_changed=0"
    Write-Output "authoritative_physical=19/19"
    Write-Output "authoritative_checksums=18/18"
    Write-Output "internal_checksums=$checksumVerified/$checksumVerified"

    if ($PrepareOnly) {
        Write-Output "package_prepared=true"
        return
    }

    $outputDirectory = Split-Path -Parent $OutputZip
    if (-not (Test-Path -LiteralPath $outputDirectory -PathType Container)) { throw "Output directory not found: $outputDirectory" }
    $outputResolved = (Resolve-Path -LiteralPath $outputDirectory).Path
    if (-not $outputResolved.StartsWith("C:\Users\thdwl\Documents\Codex", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to write package outside the requested Codex directory: $outputResolved"
    }

    $archiveStream = [IO.File]::Open($OutputZip, [IO.FileMode]::Create, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $archive = [IO.Compression.ZipArchive]::new($archiveStream, [IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            foreach ($file in ($files | Sort-Object { Get-RelativePath $_.FullName })) {
                $relative = Get-RelativePath $file.FullName
                $entry = $archive.CreateEntry("WebEditor/$relative", [IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = [DateTimeOffset]::new($file.LastWriteTimeUtc)
                $input = [IO.File]::OpenRead($file.FullName)
                $output = $entry.Open()
                try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
            }
        } finally {
            $archive.Dispose()
        }
    } finally {
        $archiveStream.Dispose()
    }

    $zipArchive = [IO.Compression.ZipFile]::OpenRead($OutputZip)
    try {
        $entries = @($zipArchive.Entries | Where-Object { -not [string]::IsNullOrEmpty($_.Name) })
        if ($entries.Count -ne $files.Count) { throw "ZIP entry count mismatch" }
        $caseKeys = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $forbiddenEntries = 0
        $entryHashes = @{}
        foreach ($entry in $entries) {
            $name = $entry.FullName.Replace("\", "/")
            if (-not $name.StartsWith("WebEditor/", [StringComparison]::Ordinal)) { throw "ZIP root violation: $name" }
            if ($name.StartsWith("/", [StringComparison]::Ordinal) -or $name -match "^[A-Za-z]:" -or $name.Split("/") -contains "..") {
                throw "ZIP path traversal or absolute path: $name"
            }
            if (-not $caseKeys.Add($name)) { throw "ZIP case-insensitive duplicate: $name" }
            $relative = $name.Substring("WebEditor/".Length)
            if (-not (Test-PackageIncluded $relative)) { $forbiddenEntries += 1 }
            $stream = $entry.Open()
            try { $entryHashes[$relative] = Get-Sha256Stream $stream } finally { $stream.Dispose() }
        }
        if ($forbiddenEntries -ne 0) { throw "Forbidden ZIP entries: $forbiddenEntries" }
        foreach ($line in Get-Content -Encoding UTF8 -LiteralPath $checksumsFull) {
            $line -match "^([0-9a-f]{64})  (.+)$" | Out-Null
            if (-not $entryHashes.ContainsKey($Matches[2]) -or $entryHashes[$Matches[2]] -ne $Matches[1]) {
                throw "ZIP internal checksum mismatch: $($Matches[2])"
            }
        }
    } finally {
        $zipArchive.Dispose()
    }

    $zipHash = Get-Sha256File $OutputZip
    $sidecar = "$OutputZip.sha256"
    [IO.File]::WriteAllText($sidecar, "$zipHash  $([IO.Path]::GetFileName($OutputZip))`n", [Text.UTF8Encoding]::new($false))
    $sidecarHash = ((Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecar).Trim().Split(" ")[0]).ToLowerInvariant()
    if ($sidecarHash -ne $zipHash) { throw "ZIP sidecar mismatch" }
    $zipInfo = Get-Item -LiteralPath $OutputZip
    Write-Output "zip_path=$($zipInfo.FullName)"
    Write-Output "zip_bytes=$($zipInfo.Length)"
    Write-Output "zip_entries=$($files.Count)"
    Write-Output "zip_sha256=$zipHash"
    Write-Output "sidecar_path=$sidecar"
    Write-Output "sidecar_matches=true"
    Write-Output "forbidden_entries=0"
    Write-Output "path_traversal=0"
    Write-Output "absolute_paths=0"
    Write-Output "case_duplicates=0"
    Write-Output "crc_or_read_errors=0"
} finally {
    $baselineArchive.Dispose()
}
