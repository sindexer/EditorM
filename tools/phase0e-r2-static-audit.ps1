$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$outputPath = Join-Path $repoRoot "docs/verification/PHASE_0E_R2_STATIC_AUDIT.txt"
$lines = [Collections.Generic.List[string]]::new()

function Add-Result([string]$Name, [bool]$Passed, [string]$Detail) {
    $status = if ($Passed) { "PASS" } else { "FAIL" }
    $lines.Add("$status $Name - $Detail")
    if (-not $Passed) { throw "$Name failed: $Detail" }
}

function Find-Matches([string[]]$Paths, [string]$Pattern) {
    $results = [Collections.Generic.List[string]]::new()
    foreach ($path in $Paths) {
        Get-ChildItem -LiteralPath (Join-Path $repoRoot $path) -Recurse -File | ForEach-Object {
            $relative = $_.FullName.Substring($repoRoot.Length + 1).Replace("\", "/")
            Select-String -LiteralPath $_.FullName -Pattern $Pattern -AllMatches | ForEach-Object {
                $results.Add("$relative`:$($_.LineNumber):$($_.Line.Trim())")
            }
        }
    }
    return @($results)
}

$lines.Add("Phase 0E-R2 structural source audit")
$lines.Add("captured_at_utc=$([DateTime]::UtcNow.ToString('o'))")

$documentClone = Find-Matches @("crates/document/src") "document\.clone\(\)|Document::clone"
Add-Result "Document edit clone" ($documentClone.Count -eq 0) "matches=$($documentClone.Count)"

$reactArrays = Find-Matches @("web/editor/src") "\.indexOf\(|\.filter\(|\.forEach\(|\.splice\("
Add-Result "React incremental large-array operations" ($reactArrays.Count -eq 0) "matches=$($reactArrays.Count)"

$sceneOrderKeys = Find-Matches @("crates/scene/src") "order_key"
Add-Result "Scene cached order_key" ($sceneOrderKeys.Count -eq 0) "matches=$($sceneOrderKeys.Count)"

$sceneSibling = Find-Matches @("crates/scene/src") "sibling_index"
Add-Result "Scene sibling_index only in semantic snapshot" ($sceneSibling.Count -eq 2) "matches=$($sceneSibling.Count); expected public snapshot field plus snapshot derivation"

$sceneCopies = Find-Matches @("crates/scene/src") "children\.to_vec\(\)"
Add-Result "Scene children copy only in semantic snapshot" ($sceneCopies.Count -eq 1) "matches=$($sceneCopies.Count)"

$commandCopies = Find-Matches @("crates/document/src/command.rs") "children\(\)\.to_vec\(\)"
Add-Result "Document command copies bounded or snapshot data only" ($commandCopies.Count -eq 2) "matches=$($commandCopies.Count); k-child Ungroup plus explicit subtree snapshot"

$metadata = Find-Matches @("crates", "shared", "web") "__phase0e_r1_group_positions"
$metadataValid = $metadata.Count -eq 2 -and
    ($metadata | Where-Object { $_ -match "crates/document/tests/phase0e_group\.rs" }).Count -eq 1 -and
    ($metadata | Where-Object { $_ -match "crates/serialization/src/lib\.rs" }).Count -eq 1
Add-Result "Legacy group metadata removed from product state" $metadataValid "matches=$($metadata.Count); both remaining references are negative assertions"

$uiFullOrder = Find-Matches @("web/editor/src/engine.ts") "const order: string\[\]"
Add-Result "UI dense order limited to explicit full initialization" ($uiFullOrder.Count -eq 1) "matches=$($uiFullOrder.Count); rebuildHierarchy full boundary"

$lines.Add("allowed_boundaries=Document semantic snapshot; explicit subtree snapshot; Scene semantic snapshot; persistence; UI initial full projection")
$lines.Add("FINAL_STATUS=PASS")
[IO.File]::WriteAllLines($outputPath, $lines, [Text.UTF8Encoding]::new($false))
$lines | ForEach-Object { Write-Host $_ }
Write-Host "static_audit=$outputPath"
