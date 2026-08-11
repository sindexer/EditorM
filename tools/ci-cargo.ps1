[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true, Mandatory = $true)]
    [string[]]$CargoArguments
)

$ErrorActionPreference = "Continue"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
& cargo @CargoArguments
$firstExit = $LASTEXITCODE
if ($firstExit -eq 0) { exit 0 }

$targetPath = [IO.Path]::GetFullPath((Join-Path $repoRoot "target"))
$expectedPrefix = [IO.Path]::GetFullPath($repoRoot) + [IO.Path]::DirectorySeparatorChar
if (-not $targetPath.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Cargo target cleanup escaped the repository"
}

Write-Warning "Cargo command failed once. Removing only the repository target cache and retrying to recover from a damaged cache."
if (Test-Path -LiteralPath $targetPath) {
    Remove-Item -LiteralPath $targetPath -Recurse -Force
}
& cargo @CargoArguments
exit $LASTEXITCODE
