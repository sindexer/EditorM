$CargoArguments = $args

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$configuredToolRoot = $env:VAE_TOOL_ROOT
$localToolRoot = if ($configuredToolRoot) {
    $configuredToolRoot
} else {
    Join-Path $workspaceRoot '.tools'
}
$localCargo = Join-Path $localToolRoot 'cargo\bin\cargo.exe'
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue

if ($cargoCommand) {
    $cargoExecutable = $cargoCommand.Source
} elseif (Test-Path -LiteralPath $localCargo) {
    $cargoExecutable = $localCargo
    $env:RUSTUP_HOME = Join-Path $localToolRoot 'rustup'
    $env:CARGO_HOME = Join-Path $localToolRoot 'cargo'

    $toolchain = Get-ChildItem -Path (Join-Path $localToolRoot 'rustup\toolchains') -Directory |
        Select-Object -First 1 -ExpandProperty FullName
    $selfContained = Join-Path $toolchain 'lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained'
    $binutils = Join-Path $localToolRoot 'msys2-binutils-2.47\mingw64\bin'
    $gnuLinker = Join-Path $selfContained 'x86_64-w64-mingw32-gcc.exe'

    if (Test-Path -LiteralPath $gnuLinker) {
        $env:Path = "$binutils;$selfContained;$env:Path"
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $gnuLinker
    }
} else {
    throw 'Cargo was not found. Install Rust 1.89.0 or set VAE_TOOL_ROOT to an isolated toolchain.'
}

Push-Location $workspaceRoot
try {
    & $cargoExecutable @CargoArguments
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
