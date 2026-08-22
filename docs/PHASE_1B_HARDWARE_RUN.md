# Phase 1B Hardware Verification Run

Gate 1B requires a Windows workstation with a real hardware GPU. The branch-preparation environment
cannot produce that evidence, so Gate 1B remains **NOT PASSED** until this runner succeeds on the
GTX 970 workstation. Software GPU output is diagnostic only and can never satisfy the Gate.

## Prerequisites

- Check out `main` with no production or harness changes after the commit that will be tested.
- Install Google Chrome with working WebGPU. Set `PHASE0E_CHROME` only when Chrome is not at
  `C:\Program Files\Google\Chrome\Application\chrome.exe`.
- Install the pinned Rust toolchain and `wasm-bindgen` 0.2.126. The existing `VAE_TOOL_ROOT` and
  `VAE_WASM_BINDGEN` overrides remain supported.
- Start in the repository root. Do not enter `web/editor` manually.

## Single Windows command

Run exactly this command from the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/run-phase1b-gate.ps1
```

The runner enters `web/editor` exactly once. There is no repeated `cd web/editor`, and users do not
need to copy a sequence of npm commands by hand.

## Recorded run identity and environment

Before validation starts, `docs/verification/PHASE_1B_GATE_RUN.json` records:

- the Gate run ID, full tested Git commit SHA, tested branch, and UTC start time;
- Windows version, Chrome executable, Node version, Rust toolchain, Cargo/Rust versions, and
  `wasm-bindgen` version;
- the initial Git status and every unexpected non-evidence path.

The runner exports `PHASE1B_GATE_RUN_ID`, `PHASE1B_GATE_SOURCE_COMMIT`, and
`PHASE1B_GATE_SOURCE_BRANCH` to every child process. Browser, pixel, benchmark, direct-WASM, and
final Gate JSON records must carry the same values. An initially dirty source tree is printed. Any
production or harness change fails the Gate; pre-existing verification artifacts are reported and
may remain dirty.

## Fail-closed execution order

The runner stops on the first failed step and records that failure. The order is fixed:

1. `tools/cargo.ps1 fmt --all -- --check`
2. `tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings`
3. `tools/cargo.ps1 build --workspace --all-targets`
4. `tools/cargo.ps1 test --workspace --all-targets`
5. `tools/build-phase0e-wasm.ps1`
6. Enter `web/editor` once and run `npm.cmd ci`.
7. Run `npm.cmd run test:browser:phase1a`.
8. Run `npm.cmd run test:browser:phase1b` through real Chrome/CDP input -> React Editor ->
   Dedicated Worker -> shipped WASM -> WebGPU -> hardware GPU.
9. Run `npm.cmd run test:direct-wasm:phase1b`.
10. Run `npm.cmd run bench:multi-drag:phase1b`.
11. Run `npm.cmd run build`.
12. Run `npm.cmd run finalize:gate:phase1b`.
13. Run `npm.cmd test` only after the generated Gate status is current.

The finalizer order is intentional. A successful browser harness creates
`PHASE_1B_BROWSER_PROOF.json`; the default Gate guard therefore runs only after the finalizer has
calculated `PHASE_1B_GATE_STATUS.json` from that proof and the other current-run evidence.

## Automatic finalization

Do not hand-edit `PHASE_1B_GATE_STATUS.json`. The finalizer reads the evidence and calculates every
item as `PASS`, `FAIL`, or `UNVERIFIED`, then derives the summary counts. `gate_conclusion` is
`PASSED` only when there are no failed or unverified items and every required item is `PASS`.

It verifies at minimum:

- a passing current-run Phase 1A browser regression;
- actual Chrome, Dedicated Worker, initialized WASM, actual WebGPU, and
  `gpu.software_renderer === false` in the Phase 1B browser proof;
- passing pixel evidence;
- snapped and unsnapped 10/100/1,000 browser benchmark series, retaining the 16.7 ms thresholds
  for 10 and 100 only;
- the direct-WASM 46/46 proof, engine benchmark, and complete runner prerequisite sequence;
- one Gate run ID, source commit, and branch across every evidence artifact.

One thousand dragged objects remain a measurement with no new Gate threshold.

## Tested commit and evidence commits

An evidence-only commit may follow the tested source commit. The finalizer accepts this only when
the tested commit is an ancestor of the current HEAD and every later committed or uncommitted path
is Gate evidence under `docs/verification/`, the Phase 1B metrics files, or Gate review evidence.
Any production or harness change after the tested commit invalidates the hardware proof and
requires a new hardware run.

## Failure handling

On failure the runner preserves the harness failure artifact, updates the run manifest, invokes the
finalizer again, and leaves `gate_conclusion` as `NOT PASSED`. Its report includes the first failing
step/assertion, related artifacts, Chrome/GPU diagnostics where available, source commit, and Gate
run ID. Assertion failures are `FAIL`; only an unavailable hardware GPU may leave GPU-dependent
items `UNVERIFIED`.

`PHASE1B_ALLOW_SOFTWARE_GPU=1` remains diagnostic-only and writes separate `*_SOFTWARE_RUN.json`
files. Such a run is never Gate evidence.

## Successful evidence set

A successful hardware run produces or refreshes at least:

- Phase 1A browser regression evidence;
- `docs/verification/PHASE_1B_BROWSER_PROOF.json`;
- `docs/verification/PHASE_1B_PIXEL_READBACK.json`;
- `docs/PHASE_1B_METRICS.json`;
- `docs/verification/phase1b-multi-selection.png`;
- `docs/verification/phase1b-marquee.png`;
- `docs/verification/phase1b-snap-guide.png`;
- `docs/verification/phase1b-aligned.png`;
- `docs/verification/phase1b-distributed.png`;
- `docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json`;
- `docs/PHASE_1B_METRICS_ENGINE_ONLY.json`;
- `docs/verification/PHASE_1B_GATE_RUN.json`;
- `docs/verification/PHASE_1B_GATE_STATUS.json`.

Success requires `FAIL = 0`, `UNVERIFIED = 0`, and `gate_conclusion = PASSED`.
