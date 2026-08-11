globalThis.__runPhase0eR3Matrix = async function runPhase0eR3Matrix() {
  const send = globalThis.__phase0eSend;
  const ui = globalThis.__phase0eR2Counters;
  if (!send || !ui) throw new Error("R3 browser matrix diagnostics are unavailable");
  const ks = [3, 30, 100, 300, 1000, 3000, 10000];
  const patterns = ["contiguous_front", "contiguous_middle", "uniform", "random_seed_0x5eed", "multiple_runs"];
  const fixtureNodeId = (index) => {
    const hex = (BigInt(index) + 1n).toString(16).padStart(32, "0");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  };
  const percentile = (values, fraction) => [...values].sort((left, right) => left - right)[Math.ceil((values.length - 1) * fraction)];
  const targetIndexes = (count, k, pattern) => {
    if (pattern === "contiguous_front") return Array.from({ length: k }, (_, index) => index + 1);
    if (pattern === "contiguous_middle") {
      const start = Math.floor((count - k) / 2) + 1;
      return Array.from({ length: k }, (_, index) => start + index);
    }
    if (pattern === "uniform") return Array.from({ length: k }, (_, index) => 1 + Math.floor(index * (count - 1) / (k - 1)));
    if (pattern === "random_seed_0x5eed") {
      let state = 0x5eed ^ count ^ k;
      const selected = new Set();
      while (selected.size < k) {
        state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
        selected.add(1 + state % count);
      }
      return [...selected].sort((left, right) => left - right);
    }
    if (k === count) return Array.from({ length: count }, (_, index) => index + 1);
    const runCount = Math.min(7, k, count - k + 1);
    const baseLength = Math.floor(k / runCount);
    let remainder = k % runCount;
    const gap = Math.floor((count - k) / (runCount + 1));
    let cursor = 1 + gap;
    const result = [];
    for (let run = 0; run < runCount; run += 1) {
      const length = baseLength + (remainder-- > 0 ? 1 : 0);
      for (let offset = 0; offset < length; offset += 1) result.push(cursor + offset);
      cursor += length + gap;
    }
    return result;
  };
  const counterDelta = (after, before) => Object.fromEntries(Object.keys(after).map((key) => [key, after[key] - (before[key] ?? 0)]));
  const engineWork = (response) => ["document", "scene"].reduce((sum, prefix) => {
    const metric = (suffix) => Number(response.metrics[`${prefix}_sequence_${suffix}`] ?? 0);
    return sum + Math.max(metric("entries_examined"), metric("rank_order_comparisons")) + metric("entries_copied") +
      metric("entries_moved_or_shifted") + metric("nodes_allocated") + Math.ceil(metric("allocated_bytes") / 8) +
      metric("tree_rebalances") +
      metric("full_scans") + metric("full_copies") + metric("dense_index_rewrites") + metric("fallback_or_rebuild_count");
  }, 0);
  const uiWork = (value) => Math.max(value.sequenceEntriesExamined, value.sequenceRankOrderComparisons) + value.sequenceEntriesCopied +
    value.sequenceEntriesMovedOrShifted + value.sequenceNodesAllocated + Math.ceil(value.sequenceAllocatedBytes / 8) +
    value.sequenceTreeRebalances +
    value.sequenceFullScans + value.sequenceFullCopies + value.sequenceDenseIndexRewrites + value.sequenceFallbackOrRebuildCount;
  const assertBounded = (response, uiDelta, label) => {
    if (!response.ok) throw new Error(`${label}: ${response.error?.code}: ${response.error?.message}`);
    const failures = [];
    for (const prefix of ["document", "scene"]) for (const suffix of ["full_scans", "full_copies", "dense_index_rewrites", "fallback_or_rebuild_count"]) {
      if (Number(response.metrics[`${prefix}_sequence_${suffix}`] ?? 0) !== 0) failures.push(`${prefix}.${suffix}`);
    }
    for (const key of ["sequenceFullScans", "sequenceFullCopies", "sequenceDenseIndexRewrites", "sequenceFallbackOrRebuildCount", "hierarchyFullRebuilds"]) {
      if (Number(uiDelta[key] ?? 0) !== 0) failures.push(`ui.${key}`);
    }
    if (Number(response.metrics.fallback_rebuild_count ?? 0) !== 0) failures.push("fallback_rebuild_count");
    if (Number(response.metrics.full_render_model_scans ?? 0) !== 0) failures.push("full_render_model_scans");
    if (Number(response.metrics.render_items_cloned ?? 0) !== 0) failures.push("render_items_cloned");
    if (Number(response.render_delta.dirty_slots ?? 0) !== 0 || Number(response.metrics.instance_upload_bytes ?? 0) !== 0) failures.push("structural_gpu_delta");
    if (failures.length) throw new Error(`${label}: ${failures.join(",")}`);
  };
  const summarize = (samples) => ({
    raw_samples_ms: samples.times,
    raw_work_samples: samples.work,
    median_ms: percentile(samples.times, 0.5),
    p95_ms: percentile(samples.times, 0.95),
    maximum_work: Math.max(...samples.work),
    maximum_sequence_depth: Math.max(...samples.depth),
    tree_rebalances: Math.max(...samples.rebalances),
  });

  const matrix = [];
  const startedAt = new Date().toISOString();
  for (const count of [10000, 100000]) {
    const fixture = count === 10000 ? "bench-b" : "bench-c";
    const loaded = await send("load_fixture", { fixture });
    if (!loaded.ok) throw new Error(`R3 browser fixture ${fixture} failed`);
    for (const k of ks) for (const pattern of patterns) {
      const reset = await send("load_fixture", { fixture });
      if (!reset.ok) throw new Error("R3 browser fixture reset " + fixture + " failed");
      const indexes = targetIndexes(count, k, pattern);
      const targets = indexes.map(fixtureNodeId);
      const groupId = fixtureNodeId(BigInt(count) * 1000n + BigInt(k) * 10n + BigInt(patterns.indexOf(pattern) + 1));
      const samples = Object.fromEntries(["group", "undo", "redo", "ungroup"].map((name) => [name, { times: [], work: [], depth: [], rebalances: [] }]));
      for (let iteration = 0; iteration < 40; iteration += 1) {
        const operations = [
          ["group", "command", { command: { kind: "group", group_id: groupId, name: "R3 Chrome matrix", targets } }],
          ["undo", "undo", {}],
          ["redo", "redo", {}],
          ["ungroup", "command", { command: { kind: "ungroup", node_id: groupId } }],
        ];
        for (const [name, type, body] of operations) {
          const beforeUi = ui();
          const started = performance.now();
          const response = await send(type, body);
          const elapsed = performance.now() - started;
          const uiDelta = counterDelta(ui(), beforeUi);
          assertBounded(response, uiDelta, `${count}/${k}/${pattern}/${name}`);
          if (iteration >= 10) {
            samples[name].times.push(elapsed);
            samples[name].work.push(engineWork(response) + uiWork(uiDelta));
            samples[name].depth.push(Math.max(Number(response.metrics.document_sequence_maximum_depth ?? 0), Number(response.metrics.scene_sequence_maximum_depth ?? 0), Number(ui().sequenceMaximumDepth ?? 0)));
            samples[name].rebalances.push(Number(response.metrics.document_sequence_tree_rebalances ?? 0) + Number(response.metrics.scene_sequence_tree_rebalances ?? 0) + Number(uiDelta.sequenceTreeRebalances ?? 0));
          }
        }
      }
      const saved = await send("save_document");
      const document = JSON.parse(saved.result.document_json);
      const root = document.document.nodes.find((node) => node.id === fixtureNodeId(0));
      if (!root || root.children.length !== count || root.children.some((value, index) => value !== fixtureNodeId(index + 1))) {
        throw new Error(`${count}/${k}/${pattern}: browser exact order mismatch`);
      }
      matrix.push({
        node_count: count, selection_count: k, pattern, warmups: 10, measured_iterations: 30,
        operations: Object.fromEntries(Object.entries(samples).map(([name, value]) => [name, summarize(value)])),
        exact_sibling_order_restored: true,
      });
    }
  }
  const entry = (count, k, pattern) => matrix.find((value) => value.node_count === count && value.selection_count === k && value.pattern === pattern);
  const thresholds = [];
  for (const count of [10000, 100000]) for (const pattern of patterns) {
    const perK = (entry(count, 3000, pattern).operations.group.maximum_work / 3000) / (entry(count, 30, pattern).operations.group.maximum_work / 30);
    thresholds.push({ name: `work_per_k_${count}_${pattern}`, value: perK, limit: 2, passed: perK <= 2 });
    const x10 = entry(count, 3000, pattern).operations.group.maximum_work / entry(count, 300, pattern).operations.group.maximum_work;
    thresholds.push({ name: `x10_work_${count}_${pattern}`, value: x10, limit: 15, passed: x10 <= 15 });
  }
  for (const k of ks) for (const pattern of patterns) {
    const ratio = entry(100000, k, pattern).operations.group.maximum_work / entry(10000, k, pattern).operations.group.maximum_work;
    thresholds.push({ name: `n_ratio_${k}_${pattern}`, value: ratio, limit: 1.35, passed: ratio <= 1.35 });
  }
  if (thresholds.some((value) => !value.passed)) throw new Error(`R3 browser thresholds failed: ${JSON.stringify(thresholds.filter((value) => !value.passed))}`);
  return {
    phase: "0E-R3", layer: "actual-chrome-worker-wasm-react-webgpu", command: "npm run test:browser:r3",
    started_at_utc: startedAt, finished_at_utc: new Date().toISOString(), fixture_creation_and_load_excluded: true,
    matrix, thresholds, all_passed: true,
  };
};
