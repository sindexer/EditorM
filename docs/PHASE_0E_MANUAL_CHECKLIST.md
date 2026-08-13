# Phase 0E 수동 검수 체크리스트

## 실행

프로젝트 루트에서 다음 한 줄을 실행한다.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-phase0e-editor.ps1
```

스크립트가 표시한 `http://127.0.0.1:<port>/` 주소를 연다. health와 필수 asset/MIME 확인이 끝나기 전에는 제품 준비 완료로 판정하지 않는다. 종료할 때는 실행 중인 PowerShell 창에서 `Ctrl+C`를 눌러 preview server를 정리한다.

## 시작 상태

- 화면 상단에 Vector Forge, Phase 0E, Undo/Redo, Group/Ungroup, Components, Restart Worker가 보인다.
- 왼쪽에 도구 rail과 Layers, 중앙에 실제 WebGPU canvas, 오른쪽에 Transform Inspector가 보인다.
- 하단 상태 표시에서 Worker + WebGPU가 준비되고 sequence, document revision, node 수가 표시된다.
- Debug panel은 접고 펼칠 수 있다.

## 직접 조작

- Layers의 `Proof Rectangle`을 선택하면 파란 selection overlay가 나타난다.
- Shift를 누른 채 다른 행을 선택하면 다중 선택된다.
- Select 도구에서 도형을 드래그하면 이동하고 Undo 항목은 정확히 1개 증가한다.
- resize handle과 rotate handle을 각각 드래그하면 크기와 회전이 바뀌고, 각 drag는 Undo 1개다.
- drag 중 Escape를 누르면 시작 상태로 완전히 복귀하고 Undo 항목은 늘지 않는다.
- Rectangle/Ellipse 도구로 canvas를 드래그하면 해당 도형이 생성되고 Layers에 나타난다.

## 구조와 속성

- 두 도형을 선택하고 Group을 누르면 Group 한 개가 선택되며 Undo 1개가 추가된다.
- Group 행을 더블클릭하면 nested editing breadcrumb가 나타난다.
- nested group 안의 child를 이동한 뒤에도 overlay와 canvas가 일치한다.
- `Exit group` 후 Group을 선택하고 Ungroup을 누르면 child가 원래 world 위치를 유지하고 Undo 1개가 추가된다.
- Layers의 눈/자물쇠 버튼으로 visibility와 lock을 바꾼다. 잠긴 도형은 canvas drag로 변경되지 않아야 한다.
- Inspector의 X/Y/Width/Height/Rotation/Opacity 값을 Enter 또는 blur로 확정한다. 유효하지 않은 값은 typed error로 표시되고 성공값으로 처리되지 않아야 한다.
- Undo와 Redo가 create, transform, visibility, lock, Group/Ungroup을 되돌리고 다시 적용한다.

## 카메라와 런타임

- Hand 도구 drag로 pan한다. 이 동작은 document revision을 바꾸지 않는다.
- 마우스 wheel로 pointer 주변을 확대/축소하고, pointer 아래 world 위치가 유지되는지 확인한다.
- Fit selection과 Fit document가 선택/문서 범위를 viewport에 맞추며 document revision을 바꾸지 않는다.
- Restart Worker 후 preview fixture가 다시 준비되고 Worker heartbeat와 GPU/overlay sequence가 다시 일치한다.

## 대규모 fixture

- Debug panel에서 10k를 연다. Layers의 mounted row 수가 viewport 범위로 제한되는지 확인한다.
- 단일 `Rectangle 0`의 X 값을 바꾼다. full UI snapshot, Layers full serialize, hierarchy rebuild가 증가하지 않고 delta node가 1 이하인지 확인한다.
- 100k를 연다. 100,001 projection node에서도 mounted row가 30 이하이고 UI가 응답하는지 확인한다.
- single-leaf 변경에서 dirty GPU instance 1, upload 48 bytes, full RenderModel clone/scan과 unrelated rebuild 0을 확인한다.

## UI와 오류 상태

- Components에서 Button, input, select, tabs, checkbox/switch, status, focus/disabled/error 상태를 확인한다.
- Tab 키로 주요 제어에 접근할 수 있고 focus ring이 보이는지 확인한다.
- tooltip과 각 icon button에 accessible name이 있는지 확인한다.
- browser console error, GPU validation error, fallback rebuild가 모두 0인지 확인한다.

## 범위 확인

marquee, snapping/guides, align/distribute, duplicate/copy/paste, scrubbing, pivot, path/text, boolean/effects, image, timeline/animation, collaboration/plugins/AI, PSD/AE, export 기능이 추가되지 않았는지 확인한다. 이 항목들은 Phase 1 이후 범위다.
