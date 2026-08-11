# Phase 0D-R1-P1 Evidence Provenance

Date: 2026-08-10 (Asia/Seoul)

## Review status and scope

The external review technically accepted the structural Phase 0D-R1 corrections. Gate 0D-R1
remained held because the R1 review package contained earlier Phase 0C/0D evidence paths that
had been overwritten by later verification runs.

Phase 0D-R1-P1 changes evidence provenance and packaging only. Product source, shared render
contracts, WASM output, browser deployment output, tests, benchmarks, and runtime behavior were
not changed. Phase 0E, React, and Wanted Design System product UI were not started.

## Trusted source archive

- Archive: `visual_authoring_engine_phase0d_review_2026-08-08.zip`
- Size: 46,848,080 bytes
- SHA-256: `34b1f81245beec3807dd886a7256deecaf51f168bfc4187a6e817de3288f6afe`
- Verification: the archive SHA-256 was calculated directly before extraction and matched.

## Byte-for-byte restored earlier-phase evidence

The files below were extracted from the trusted archive. They were not regenerated or replaced
with current test results.

| Earlier phase | Path | SHA-256 |
| --- | --- | --- |
| Phase 0D | `docs/verification/PHASE_0D_BROWSER_PROOF.json` | `50007ae056a1d4be0b20d385e0d2c1068c9439a789c0a9aa485faa4762be5513` |
| Phase 0D | `docs/verification/PHASE_0D_VERIFICATION.txt` | `6185f1c8556ba3f476cdc6c4e49f194125dac7cdbd167ae4ed51d56bfb3f38e5` |
| Phase 0D | `docs/verification/phase0d-preview.png` | `31147497e35b66391259aa10c6f3e91b92a9d99f252798bd0f534d420ec0570f` |
| Phase 0C | `docs/PHASE_0C_METRICS.json` | `7462465dddb58356e8b1dfe21cb4610106d99c42f775c97c8e9a56c836289730` |
| Phase 0C | `docs/verification/PHASE_0C_VERIFICATION.txt` | `32f267d12588bf0189fce4cfc29530c7a91799a9be0084310bdf0877989b093d` |

`docs/PHASE_0D_METRICS.json` was already byte-identical to the trusted archive and was not
changed.

## Preserved Phase 0D-R1 evidence

The existing R1 evidence was not regenerated or edited.

| Path | SHA-256 |
| --- | --- |
| `docs/PHASE_0D_R1_METRICS.json` | `b6cc7adfbc0723bd2f12ab44b80e1a70e90afe85117d64c70a3f718cfe5873fa` |
| `docs/verification/PHASE_0D_R1_VERIFICATION.txt` | `fa8f181fcbd14c0f316484b47d10d4348d0a6b225a501d0f4c25f18934438e1c` |
| `docs/verification/PHASE_0D_R1_BROWSER_PROOF.json` | `a30fce172312980109992bc1414ade0612bc6016d19fb9c32e1472fed82ffeaf` |
| `docs/verification/PHASE_0D_R1_PIXEL_READBACK.json` | `2c51c66ddf934a9a0aab4c8e576c7f9b3064df7ede93bc324f7167bee5144702` |
| `docs/verification/phase0d-r1-preview.png` | `d1f019b2cf2cabd73e5312135efe281a864d95307d1a4daeade0d17e29f09fc2` |
| `docs/REVIEW_PACKET_0D_R1.md` | `718868136841cdc63fa373018dedb640af4911257b51a27955a9c02c73f20173` |

The existing R1 ZIP
`visual_authoring_engine_phase0d_r1_review_2026-08-09.zip` and its sidecar were preserved and
not overwritten or deleted.

## Execution and evidence statement

No Rust, WASM, WebGPU, browser, benchmark, or product test was run for this provenance-only
repair. No stored JSON was represented or counted as a new execution result. Validation was
limited to archive/file hashes, byte comparisons, authoritative-package checks, manifest checks,
and ZIP-integrity/safety checks required for Phase 0D-R1-P1 packaging.

Gate 0D-R1 is not declared approved by this document. Work stops after producing the P1 review
ZIP and SHA-256 sidecar.
