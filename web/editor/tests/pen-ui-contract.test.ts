import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const workspace = path.resolve(import.meta.dirname, "../../..");
const app = readFileSync(path.join(workspace, "web/editor/src/App.tsx"), "utf8");

describe("Phase 2A Pen UI contract", () => {
  test("Pen is a keyboard-accessible graphics tool above the canvas", () => {
    expect(app).toContain('{ id: "pen", label: "Pen", shortcut: "P", icon: PenTool }');
    expect(app).toContain('className="graphics-toolbox"');
    expect(app).toContain('aria-label="Graphics tools"');
    expect(app).toContain('testId={`tool-${entry.id}`}');
  });

  test("the UI sends gestures and never computes persistent handles", () => {
    expect(app).toContain('kind: "create_path_from_pen"');
    expect(app).toContain('kind: "set_path_from_pen"');
    expect(app).toContain("gestures: draft.anchors");
    expect(app).not.toContain("handle_in");
    expect(app).not.toContain("handle_out");
  });

  test("one Pen session is one atomic transaction", () => {
    expect(app).toContain('engine.send("begin_transaction")');
    expect(app).toContain('engine.send("commit_transaction")');
    expect(app).toContain('engine.send("rollback_transaction")');
    expect(app).toContain('event.key === "Enter" && penDraft.current');
    expect(app).toContain('event.key === "Escape" && penDraft.current');
  });

  test("browser proof state exposes draft progress for E2E synchronization", () => {
    expect(app).toContain("pen_draft: penDraft.current");
    expect(app).toContain("anchor_count: penDraft.current.anchors.length");
    expect(app).toContain("created: penDraft.current.created");
  });
});

