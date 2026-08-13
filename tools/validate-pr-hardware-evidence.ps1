[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BaseRef,
    [string]$HeadRef = "HEAD"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$validator = Join-Path $PSScriptRoot "validate-hardware-evidence.ps1"
$candidates = @(git -C $repoRoot diff --name-only --diff-filter=A "$BaseRef...$HeadRef" -- docs/verification)
if ($LASTEXITCODE -ne 0) { throw "Unable to enumerate added hardware evidence" }

$validated = [Collections.Generic.List[object]]::new()
$proofCandidates = 0
$rejectedCandidates = 0
foreach ($candidate in $candidates) {
    if ($candidate -notmatch "\.json$") { continue }
    $full = Join-Path $repoRoot $candidate
    try { $json = Get-Content -Raw -LiteralPath $full | ConvertFrom-Json } catch { continue }
    $proofKind = $json.PSObject.Properties["proof_kind"]
    if ($null -eq $proofKind -or $proofKind.Value -ne "actual-hardware-browser") { continue }
    $proofCandidates++
    $hash = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToLowerInvariant()
    try {
        $resultText = (& $validator -RepositoryRoot $repoRoot -EvidencePath $candidate -EvidenceSha256 $hash 2>$null | Out-String)
        $result = $resultText | ConvertFrom-Json
        if ($result.all_passed -ne $true) { throw "Hardware evidence did not pass" }
        $validated.Add($result)
    }
    catch {
        $rejectedCandidates++
    }
}

if ($validated.Count -eq 0) {
    throw "Renderer/GPU changes require a newly added, tracked, fresh actual-hardware browser proof (candidates: $proofCandidates; rejected: $rejectedCandidates)"
}

[pscustomobject]@{
    validated_proofs = $validated.Count
    paths = @($validated | ForEach-Object { $_.evidence_path })
    all_passed = $true
} | ConvertTo-Json -Depth 4
