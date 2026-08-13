$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$repoRoot = Split-Path -Parent $PSScriptRoot
$parent = Split-Path -Parent $repoRoot
$baselineZip = Join-Path $parent "visual_authoring_engine_phase0e_review_2026-08-10.zip"
$expectedBaselineHash = "2e1481e7b6da938872c3e75f2726fdbaeb3d28a47b8e72dc28c97a3fcac9b60c"
$zipName = "visual_authoring_engine_phase0e_r1_review_2026-08-10.zip"
$zipPath = Join-Path $parent $zipName
$sidecarPath = "$zipPath.sha256"
$scratch = Join-Path $env:TEMP ("phase0e-r1-package-" + [Guid]::NewGuid().ToString("N"))
$baselineExtract = Join-Path $scratch "baseline"
$packageParent = Join-Path $scratch "package"
$stageRoot = Join-Path $packageParent "WebEditor"

function Get-RelativePath([string]$Root, [string]$Path) {
    return $Path.Substring($Root.Length + 1).Replace("\", "/")
}

function Test-Included([string]$RelativePath) {
    $path = $RelativePath.Replace("\", "/")
    if ($path -match "(^|/)(target|node_modules|\.tools|\.git|\.idea|\.vscode|dist|coverage|\.cache)(/|$)") { return $false }
    if ($path -match "(^|/)(Thumbs\.db|\.DS_Store)$") { return $false }
    if ($path -match "(^|/)\.env($|\.)" -or $path -match "\.(pem|pfx|key)$") { return $false }
    if ($path -match "\.tsbuildinfo$") { return $false }
    if ($path -match "^tools/.*codemod.*\.mjs$") { return $false }
    if ($path -match "^tools/phase0e_r1_.*\.mjs$") { return $false }
    if ($path -in @(
        "docs/verification/phase0e-default.png",
        "docs/verification/phase0e-selection.png",
        "docs/verification/phase0e-nested-group.png",
        "docs/verification/PHASE_0E_R1_BROWSER_FAILURE.json"
    )) { return $false }
    if ($path -match "\.(zip|tmp|log)$" -or $path -match "\.zip\.sha256$") { return $false }
    return $true
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

if (-not (Test-Path -LiteralPath $baselineZip -PathType Leaf)) { throw "Missing baseline ZIP: $baselineZip" }
if (Test-Path -LiteralPath $zipPath) { throw "Refusing to overwrite existing R1 ZIP: $zipPath" }
if (Test-Path -LiteralPath $sidecarPath) { throw "Refusing to overwrite existing R1 sidecar: $sidecarPath" }

$baselineHash = (Get-FileHash -LiteralPath $baselineZip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($baselineHash -ne $expectedBaselineHash) { throw "Baseline ZIP SHA-256 mismatch: $baselineHash" }

New-Item -ItemType Directory -Force -Path $baselineExtract, $stageRoot | Out-Null
try {
    [IO.Compression.ZipFile]::ExtractToDirectory($baselineZip, $baselineExtract)
    $baselineRoot = Join-Path $baselineExtract "WebEditor"
    if (-not (Test-Path -LiteralPath $baselineRoot -PathType Container)) { throw "Baseline ZIP root is not WebEditor/" }

    $baselineMap = Get-HashMap $baselineRoot
    $currentMap = Get-HashMap $repoRoot

    $deleted = @($baselineMap.Keys | Where-Object { -not $currentMap.ContainsKey($_) } | Sort-Object)
    if ($deleted.Count -ne 0) { throw "Deleted baseline files detected: $($deleted -join ', ')" }

    $changedBaselineDocs = @($baselineMap.Keys | Where-Object {
        $_.StartsWith("docs/", [StringComparison]::OrdinalIgnoreCase) -and
        $currentMap.ContainsKey($_) -and $currentMap[$_] -ne $baselineMap[$_]
    } | Sort-Object)
    if ($changedBaselineDocs.Count -ne 0) { throw "Existing Phase 0E/prior docs or evidence changed: $($changedBaselineDocs -join ', ')" }

    $authoritativePrefix = "visual_authoring_engine_codex_package/"
    $authoritativePaths = @($baselineMap.Keys | Where-Object { $_.StartsWith($authoritativePrefix, [StringComparison]::OrdinalIgnoreCase) } | Sort-Object)
    if ($authoritativePaths.Count -ne 19) { throw "Expected 19 authoritative files in baseline, got $($authoritativePaths.Count)" }
    $authoritativeChanged = @($authoritativePaths | Where-Object {
        -not $currentMap.ContainsKey($_) -or $currentMap[$_] -ne $baselineMap[$_]
    })
    if ($authoritativeChanged.Count -ne 0) { throw "Authoritative files changed: $($authoritativeChanged -join ', ')" }
    $authoritativeSumsPassed = Assert-AuthoritativeSums (Join-Path $repoRoot "visual_authoring_engine_codex_package")

    $added = @($currentMap.Keys | Where-Object { -not $baselineMap.ContainsKey($_) } | Sort-Object)
    if ($added -notcontains "docs/PHASE_0E_R1_CHANGE_MANIFEST.json") {
        $added = @($added + "docs/PHASE_0E_R1_CHANGE_MANIFEST.json" | Sort-Object)
    }
    $modified = @($currentMap.Keys | Where-Object {
        $baselineMap.ContainsKey($_) -and $currentMap[$_] -ne $baselineMap[$_]
    } | Sort-Object)
    if ($modified -notcontains "CHECKSUMS.sha256") {
        $modified = @($modified + "CHECKSUMS.sha256" | Sort-Object)
    }

    $manifest = [ordered]@{
        phase = "0E-R1"
        generated_at_utc = [DateTime]::UtcNow.ToString("o")
        baseline = [ordered]@{
            archive = Split-Path -Leaf $baselineZip
            sha256 = $baselineHash
            project_files = $baselineMap.Count
        }
        counts = [ordered]@{
            added = $added.Count
            modified = $modified.Count
            deleted = $deleted.Count
        }
        added = $added
        modified = $modified
        deleted = $deleted
        existing_baseline_docs_or_evidence_changed = $changedBaselineDocs.Count
        authoritative_files_byte_identical = "$($authoritativePaths.Count)/19"
        authoritative_sha256sums_passed = "$authoritativeSumsPassed/18"
        phase1_started = $false
    }
    $manifestPath = Join-Path $repoRoot "docs/PHASE_0E_R1_CHANGE_MANIFEST.json"
    [IO.File]::WriteAllText($manifestPath, (($manifest | ConvertTo-Json -Depth 8) + "`n"), [Text.UTF8Encoding]::new($false))

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
    [IO.File]::WriteAllLines($stageChecksums, $checksumLines, [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath $stageChecksums -Destination (Join-Path $repoRoot "CHECKSUMS.sha256") -Force

    $internalPassed = 0
    foreach ($line in Get-Content -LiteralPath $stageChecksums) {
        if ($line -notmatch "^([0-9a-f]{64})\s{2}(.+)$") { throw "Invalid internal checksum line: $line" }
        $path = Join-Path $stageRoot $Matches[2]
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $Matches[1]) { throw "Internal checksum mismatch: $($Matches[2])" }
        $internalPassed++
    }
    if ($internalPassed -ne $checksumLines.Count) { throw "Internal checksum count mismatch" }

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
            $_ -match "\.tsbuildinfo$" -or $_ -match "\.zip($|\.)" -or $_ -match "^WebEditor/tools/.*codemod.*\.mjs$" -or
            $_ -match "^WebEditor/tools/phase0e_r1_.*\.mjs$" -or $_ -match "(^|/)\.env($|\.)" -or $_ -match "\.(pem|pfx|key)$"
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
        $entryCount = $readArchive.Entries.Count
    } finally {
        $readArchive.Dispose()
    }

    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($sidecarPath, "$zipHash  $zipName`n", [Text.UTF8Encoding]::new($false))
    $sidecarParts = (Get-Content -Raw -LiteralPath $sidecarPath).Trim() -split "\s+", 2
    if ($sidecarParts[0] -ne $zipHash -or $sidecarParts[1] -ne $zipName) { throw "ZIP sidecar verification failed" }

    [pscustomobject]@{
        baseline_sha256 = $baselineHash
        added = $added.Count
        modified = $modified.Count
        deleted = $deleted.Count
        existing_baseline_docs_or_evidence_changed = $changedBaselineDocs.Count
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
    } | ConvertTo-Json -Depth 5
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
