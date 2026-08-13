$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.Security

$repoRoot = Split-Path -Parent $PSScriptRoot
$parent = Split-Path -Parent $repoRoot
$baselineZip = Join-Path $parent "visual_authoring_engine_phase0e_r1_review_2026-08-10.zip"
$expectedBaselineHash = "07d6ba27f27c32b89474b098e0e1f3045bbd2d4511eb7951d1e7d03d3bafe3db"
$zipName = "visual_authoring_engine_phase0e_r2_review_2026-08-11.zip"
$zipPath = Join-Path $parent $zipName
$sidecarPath = "$zipPath.sha256"
$scratch = Join-Path $env:TEMP ("phase0e-r2-package-" + [Guid]::NewGuid().ToString("N"))
$baselineExtract = Join-Path $scratch "baseline"
$stageRoot = Join-Path $scratch "package/WebEditor"
$utf8 = [Text.UTF8Encoding]::new($false)

function Get-RelativePath([string]$Root, [string]$Path) {
    return $Path.Substring($Root.Length + 1).Replace("\", "/")
}

function Get-ExclusionCategory([string]$RelativePath) {
    $path = $RelativePath.Replace("\", "/")
    if ($path -match "(^|/)target(/|$)") { return "target" }
    if ($path -match "(^|/)node_modules(/|$)") { return "node_modules" }
    if ($path -match "(^|/)\.tools(/|$)") { return ".tools" }
    if ($path -match "(^|/)\.git(/|$)") { return ".git" }
    if ($path -match "(^|/)(\.idea|\.vscode|coverage|\.cache)(/|$)") { return "ide_or_cache" }
    if ($path -match "(^|/)dist(/|$)") { return "dist" }
    if ($path -match "^tools/.*codemod.*\.mjs$" -or $path -match "^tools/phase0e_r[12]_.*\.mjs$") { return "work_only_codemods" }
    if ($path -in @(
        "docs/verification/phase0e-default.png",
        "docs/verification/phase0e-selection.png",
        "docs/verification/phase0e-nested-group.png",
        "docs/verification/PHASE_0E_R1_BROWSER_FAILURE.json",
        "docs/verification/PHASE_0E_R2_BROWSER_FAILURE.json"
    )) { return "obsolete_or_failure_evidence" }
    if ($path -match "\.tsbuildinfo$") { return "typescript_cache" }
    if ($path -match "\.(zip|tmp|log)$" -or $path -match "\.zip\.sha256$") { return "nested_or_temporary" }
    if ($path -match "(^|/)(Thumbs\.db|\.DS_Store)$") { return "os_metadata" }
    if ($path -match "(^|/)\.env($|\.)" -or $path -match "\.(pem|pfx|key)$") { return "credential_like" }
    return $null
}

function Test-Included([string]$RelativePath) {
    return $null -eq (Get-ExclusionCategory $RelativePath)
}

function Get-HashMap([string]$Root) {
    $map = @{}
    Get-ChildItem -LiteralPath $Root -Recurse -File -Force | ForEach-Object {
        $relative = Get-RelativePath $Root $_.FullName
        if (Test-Included $relative) {
            $map[$relative] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    return $map
}

function Assert-AuthoritativeSums([string]$AuthoritativeRoot) {
    $sumPath = Join-Path $AuthoritativeRoot "SHA256SUMS.txt"
    $lines = @(Get-Content -LiteralPath $sumPath | Where-Object { $_.Trim().Length -gt 0 })
    $passed = 0
    foreach ($line in $lines) {
        if ($line -notmatch "^([0-9a-fA-F]{64})\s{2}(.+)$") { throw "Invalid authoritative checksum line: $line" }
        $expected = $Matches[1].ToLowerInvariant()
        $path = Join-Path $AuthoritativeRoot $Matches[2]
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing authoritative file: $path" }
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) { throw "Authoritative checksum mismatch: $path" }
        $passed++
    }
    if ($passed -ne 18) { throw "Expected 18 authoritative checksum entries, got $passed" }
    return $passed
}

function Get-ZipEntryHash([IO.Compression.ZipArchiveEntry]$Entry) {
    $sha = [Security.Cryptography.SHA256]::Create()
    $stream = $Entry.Open()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
    } finally {
        $stream.Dispose()
        $sha.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $baselineZip -PathType Leaf)) { throw "Missing baseline ZIP: $baselineZip" }
if (Test-Path -LiteralPath $zipPath) { throw "Refusing to overwrite existing R2 ZIP: $zipPath" }
if (Test-Path -LiteralPath $sidecarPath) { throw "Refusing to overwrite existing R2 sidecar: $sidecarPath" }

$baselineHash = (Get-FileHash -LiteralPath $baselineZip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($baselineHash -ne $expectedBaselineHash) { throw "Baseline ZIP SHA-256 mismatch: $baselineHash" }

New-Item -ItemType Directory -Force -Path $baselineExtract, $stageRoot | Out-Null
try {
    [IO.Compression.ZipFile]::ExtractToDirectory($baselineZip, $baselineExtract)
    $baselineRoot = Join-Path $baselineExtract "WebEditor"
    if (-not (Test-Path -LiteralPath $baselineRoot -PathType Container)) { throw "Baseline ZIP root is not WebEditor/" }

    $baselineMap = Get-HashMap $baselineRoot
    $currentMap = Get-HashMap $repoRoot
    $comparisonKeys = @($baselineMap.Keys | Where-Object { $_ -ne "CHECKSUMS.sha256" })
    $currentKeys = @($currentMap.Keys | Where-Object { $_ -ne "CHECKSUMS.sha256" })

    $sourceDeleted = @($comparisonKeys | Where-Object { -not $currentMap.ContainsKey($_) } | Sort-Object)
    if ($sourceDeleted.Count -ne 0) { throw "Deleted baseline source files detected: $($sourceDeleted -join ', ')" }

    $changedPriorEvidence = @($comparisonKeys | Where-Object {
        $_.StartsWith("docs/", [StringComparison]::OrdinalIgnoreCase) -and
        -not ($_.StartsWith("docs/PHASE_0E_R2", [StringComparison]::OrdinalIgnoreCase)) -and
        $_ -ne "docs/REVIEW_PACKET_0E_R2.md" -and
        -not ($_.StartsWith("docs/verification/PHASE_0E_R2", [StringComparison]::OrdinalIgnoreCase)) -and
        -not ($_.StartsWith("docs/verification/phase0e-r2-", [StringComparison]::OrdinalIgnoreCase)) -and
        -not ($_.StartsWith("docs/adr/ADR-035", [StringComparison]::OrdinalIgnoreCase)) -and
        -not ($_.StartsWith("docs/adr/ADR-036", [StringComparison]::OrdinalIgnoreCase)) -and
        -not ($_.StartsWith("docs/adr/ADR-037", [StringComparison]::OrdinalIgnoreCase)) -and
        -not ($_.StartsWith("docs/adr/ADR-038", [StringComparison]::OrdinalIgnoreCase)) -and
        $currentMap.ContainsKey($_) -and $currentMap[$_] -ne $baselineMap[$_]
    } | Sort-Object)
    if ($changedPriorEvidence.Count -ne 0) { throw "Prior docs/evidence changed: $($changedPriorEvidence -join ', ')" }

    $authoritativePrefix = "visual_authoring_engine_codex_package/"
    $authoritativePaths = @($baselineMap.Keys | Where-Object { $_.StartsWith($authoritativePrefix, [StringComparison]::OrdinalIgnoreCase) } | Sort-Object)
    if ($authoritativePaths.Count -ne 19) { throw "Expected 19 authoritative files in baseline, got $($authoritativePaths.Count)" }
    $authoritativeChanged = @($authoritativePaths | Where-Object {
        -not $currentMap.ContainsKey($_) -or $currentMap[$_] -ne $baselineMap[$_]
    })
    if ($authoritativeChanged.Count -ne 0) { throw "Authoritative files changed: $($authoritativeChanged -join ', ')" }
    $authoritativeSumsPassed = Assert-AuthoritativeSums (Join-Path $repoRoot "visual_authoring_engine_codex_package")

    $sourceAdded = @($currentKeys | Where-Object { -not $baselineMap.ContainsKey($_) } | Sort-Object)
    if ($sourceAdded -notcontains "docs/PHASE_0E_R2_CHANGE_MANIFEST.json") {
        $sourceAdded = @($sourceAdded + "docs/PHASE_0E_R2_CHANGE_MANIFEST.json" | Sort-Object)
    }
    $sourceModified = @($comparisonKeys | Where-Object {
        $currentMap.ContainsKey($_) -and $currentMap[$_] -ne $baselineMap[$_]
    } | Sort-Object)

    $excluded = @{}
    Get-ChildItem -LiteralPath $repoRoot -Recurse -File -Force | ForEach-Object {
        $relative = Get-RelativePath $repoRoot $_.FullName
        $category = Get-ExclusionCategory $relative
        if ($null -ne $category) {
            if (-not $excluded.ContainsKey($category)) { $excluded[$category] = 0 }
            $excluded[$category]++
        }
    }
    $excludedTotal = ($excluded.Values | Measure-Object -Sum).Sum
    if ($null -eq $excludedTotal) { $excludedTotal = 0 }

    $physicalAdded = @($sourceAdded)
    $physicalModified = @($sourceModified + "CHECKSUMS.sha256" | Sort-Object -Unique)
    $manifest = [ordered]@{
        phase = "0E-R2"
        generated_at_utc = [DateTime]::UtcNow.ToString("o")
        baseline = [ordered]@{
            archive = Split-Path -Leaf $baselineZip
            bytes = (Get-Item -LiteralPath $baselineZip).Length
            sha256 = $baselineHash
            project_checksum_entries = $baselineMap.Count
        }
        source_project_comparison = [ordered]@{
            counts = [ordered]@{ added = $sourceAdded.Count; modified = $sourceModified.Count; deleted = $sourceDeleted.Count }
            added = $sourceAdded
            modified = $sourceModified
            deleted = $sourceDeleted
        }
        physical_review_zip_comparison = [ordered]@{
            counts = [ordered]@{ added = $physicalAdded.Count; modified = $physicalModified.Count; deleted = $sourceDeleted.Count }
            added = $physicalAdded
            modified = $physicalModified
            deleted = $sourceDeleted
        }
        excluded_generated_artifacts = [ordered]@{
            total_files = [int]$excludedTotal
            categories = [ordered]@{}
        }
        prior_evidence_changed = $changedPriorEvidence.Count
        authoritative_files_byte_identical = "$($authoritativePaths.Count)/19"
        authoritative_sha256sums_passed = "$authoritativeSumsPassed/18"
        phase1_started = $false
    }
    foreach ($category in $excluded.Keys | Sort-Object) {
        $manifest.excluded_generated_artifacts.categories[$category] = $excluded[$category]
    }
    $manifestPath = Join-Path $repoRoot "docs/PHASE_0E_R2_CHANGE_MANIFEST.json"
    [IO.File]::WriteAllText($manifestPath, (($manifest | ConvertTo-Json -Depth 10) + "`n"), $utf8)

    $files = @(Get-ChildItem -LiteralPath $repoRoot -Recurse -File -Force | Where-Object {
        $relative = Get-RelativePath $repoRoot $_.FullName
        (Test-Included $relative) -and $relative -ne "CHECKSUMS.sha256"
    } | Sort-Object { Get-RelativePath $repoRoot $_.FullName })
    foreach ($file in $files) {
        $relative = Get-RelativePath $repoRoot $file.FullName
        $destination = Join-Path $stageRoot $relative
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $destination
    }

    $checksumLines = [Collections.Generic.List[string]]::new()
    Get-ChildItem -LiteralPath $stageRoot -Recurse -File -Force | Sort-Object { Get-RelativePath $stageRoot $_.FullName } | ForEach-Object {
        $relative = Get-RelativePath $stageRoot $_.FullName
        $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $checksumLines.Add("$hash  $relative")
    }
    $stageChecksums = Join-Path $stageRoot "CHECKSUMS.sha256"
    [IO.File]::WriteAllLines($stageChecksums, $checksumLines, $utf8)
    Copy-Item -LiteralPath $stageChecksums -Destination (Join-Path $repoRoot "CHECKSUMS.sha256") -Force

    $archive = [IO.Compression.ZipFile]::Open($zipPath, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $null = $archive.CreateEntry("WebEditor/")
        Get-ChildItem -LiteralPath $stageRoot -Recurse -File -Force | Sort-Object { Get-RelativePath $stageRoot $_.FullName } | ForEach-Object {
            $relative = Get-RelativePath $stageRoot $_.FullName
            $entry = $archive.CreateEntry("WebEditor/$relative", [IO.Compression.CompressionLevel]::Optimal)
            $input = [IO.File]::OpenRead($_.FullName)
            $output = $entry.Open()
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        }
    } finally {
        $archive.Dispose()
    }

    $readArchive = [IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $names = @($readArchive.Entries | ForEach-Object { $_.FullName })
        $invalidPaths = @($names | Where-Object {
            $_ -notmatch "^WebEditor/" -or $_ -match "(^|/)\.\.(/|$)" -or $_.StartsWith("/") -or $_ -match "^[A-Za-z]:" -or $_ -match "\\"
        })
        $caseDuplicates = @($names | Group-Object { $_.ToLowerInvariant() } | Where-Object { $_.Count -gt 1 })
        $forbidden = @($names | Where-Object {
            $_ -match "(^|/)(target|node_modules|\.tools|\.git|\.idea|\.vscode|dist|coverage|\.cache)(/|$)" -or
            $_ -match "\.tsbuildinfo$" -or $_ -match "\.zip($|\.)" -or
            $_ -match "^WebEditor/tools/.*codemod.*\.mjs$" -or $_ -match "^WebEditor/tools/phase0e_r[12]_.*\.mjs$" -or
            $_ -match "(^|/)\.env($|\.)" -or $_ -match "\.(pem|pfx|key)$"
        })
        if ($invalidPaths.Count -ne 0) { throw "Invalid ZIP paths: $($invalidPaths -join ', ')" }
        if ($caseDuplicates.Count -ne 0) { throw "Case-insensitive duplicate ZIP paths detected" }
        if ($forbidden.Count -ne 0) { throw "Forbidden ZIP entries: $($forbidden -join ', ')" }

        $crcReadErrors = 0
        foreach ($entry in $readArchive.Entries | Where-Object { $_.Name.Length -gt 0 }) {
            try {
                $stream = $entry.Open()
                try {
                    $buffer = New-Object byte[] 65536
                    while ($stream.Read($buffer, 0, $buffer.Length) -gt 0) { }
                } finally { $stream.Dispose() }
            } catch { $crcReadErrors++ }
        }
        if ($crcReadErrors -ne 0) { throw "ZIP CRC/read errors: $crcReadErrors" }

        $checksumEntry = $readArchive.GetEntry("WebEditor/CHECKSUMS.sha256")
        if ($null -eq $checksumEntry) { throw "ZIP CHECKSUMS.sha256 is missing" }
        $reader = [IO.StreamReader]::new($checksumEntry.Open(), [Text.Encoding]::UTF8)
        try { $insideLines = @($reader.ReadToEnd() -split "`r?`n" | Where-Object { $_.Length -gt 0 }) } finally { $reader.Dispose() }
        $internalPassed = 0
        foreach ($line in $insideLines) {
            if ($line -notmatch "^([0-9a-f]{64})\s{2}(.+)$") { throw "Invalid internal checksum line: $line" }
            $entry = $readArchive.GetEntry("WebEditor/$($Matches[2])")
            if ($null -eq $entry) { throw "Checksum target missing from ZIP: $($Matches[2])" }
            if ((Get-ZipEntryHash $entry) -ne $Matches[1]) { throw "Internal ZIP checksum mismatch: $($Matches[2])" }
            $internalPassed++
        }
        if ($internalPassed -ne $checksumLines.Count) { throw "Internal checksum count mismatch" }
        $entryCount = $readArchive.Entries.Count
    } finally {
        $readArchive.Dispose()
    }

    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($sidecarPath, "$zipHash  $zipName`n", $utf8)
    $sidecarParts = (Get-Content -Raw -LiteralPath $sidecarPath).Trim() -split "\s+", 2
    if ($sidecarParts[0] -ne $zipHash -or $sidecarParts[1] -ne $zipName) { throw "ZIP sidecar verification failed" }

    [pscustomobject]@{
        baseline_sha256 = $baselineHash
        source_added = $sourceAdded.Count
        source_modified = $sourceModified.Count
        source_deleted = $sourceDeleted.Count
        physical_added = $physicalAdded.Count
        physical_modified = $physicalModified.Count
        physical_deleted = $sourceDeleted.Count
        excluded_generated_files = [int]$excludedTotal
        prior_evidence_changed = $changedPriorEvidence.Count
        authoritative_byte_identical = "$($authoritativePaths.Count)/19"
        authoritative_sha256sums = "$authoritativeSumsPassed/18"
        zip_path = $zipPath
        zip_bytes = (Get-Item -LiteralPath $zipPath).Length
        zip_entries = $entryCount
        project_files = $entryCount - 1
        internal_checksums = "$internalPassed/$internalPassed"
        forbidden_entries = 0
        invalid_paths = 0
        case_duplicates = 0
        crc_read_errors = 0
        zip_sha256 = $zipHash
        sidecar_path = $sidecarPath
        phase1_started = $false
    } | ConvertTo-Json -Depth 6
} catch {
    if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
    if (Test-Path -LiteralPath $sidecarPath) { Remove-Item -LiteralPath $sidecarPath -Force }
    throw
} finally {
    if (Test-Path -LiteralPath $scratch) {
        $resolvedScratch = (Resolve-Path -LiteralPath $scratch).Path
        $resolvedTemp = (Resolve-Path -LiteralPath $env:TEMP).Path
        if (-not $resolvedScratch.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove package scratch outside TEMP: $resolvedScratch"
        }
        Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
    }
}
