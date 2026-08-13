export interface SequenceWork {
  entriesExamined: number;
  entriesCopied: number;
  entriesMovedOrShifted: number;
  rankOrderComparisons: number;
  sequenceNodesAllocated: number;
  allocatedBytes: number;
  treeRebalances: number;
  maximumSequenceDepth: number;
  fallbackOrRebuildCount: number;
  fullSequenceScans: number;
  fullSequenceCopies: number;
  denseIndexRewrites: number;
}

export function emptySequenceWork(): SequenceWork {
  return {
    entriesExamined: 0,
    entriesCopied: 0,
    entriesMovedOrShifted: 0,
    rankOrderComparisons: 0,
    sequenceNodesAllocated: 0,
    allocatedBytes: 0,
    treeRebalances: 0,
    maximumSequenceDepth: 0,
    fallbackOrRebuildCount: 0,
    fullSequenceScans: 0,
    fullSequenceCopies: 0,
    denseIndexRewrites: 0,
  };
}

export function addSequenceWork(target: SequenceWork, update: SequenceWork): void {
  target.entriesExamined += update.entriesExamined;
  target.entriesCopied += update.entriesCopied;
  target.entriesMovedOrShifted += update.entriesMovedOrShifted;
  target.rankOrderComparisons += update.rankOrderComparisons;
  target.sequenceNodesAllocated += update.sequenceNodesAllocated;
  target.allocatedBytes += update.allocatedBytes;
  target.treeRebalances += update.treeRebalances;
  target.maximumSequenceDepth = Math.max(target.maximumSequenceDepth, update.maximumSequenceDepth);
  target.fallbackOrRebuildCount += update.fallbackOrRebuildCount;
  target.fullSequenceScans += update.fullSequenceScans;
  target.fullSequenceCopies += update.fullSequenceCopies;
  target.denseIndexRewrites += update.denseIndexRewrites;
}

class RankedNode {
  left: RankedNode | null = null;
  right: RankedNode | null = null;
  parent: RankedNode | null = null;
  size = 1;
  height = 1;

  constructor(public value: string) {}
}

export type SequenceFragment = RankedNode | null;

/** ID-independent implicit order-statistic AVL with worst-case O(log N) rank edits. */
export class RankedSequence {
  private root: RankedNode | null = null;
  private readonly byValue = new Map<string, RankedNode>();

  static from(values: Iterable<string>, work: SequenceWork): RankedSequence {
    const ordered = [...values];
    const result = new RankedSequence();
    work.entriesExamined += ordered.length;
    work.entriesCopied += ordered.length;
    work.entriesMovedOrShifted += ordered.length;
    work.sequenceNodesAllocated += ordered.length;
    work.allocatedBytes += ordered.length * 80;
    result.root = buildBalanced(ordered, 0, ordered.length, null, result.byValue);
    recordDepth(result.root, work);
    return result;
  }

  get length(): number {
    return nodeSize(this.root);
  }

  get maximumDepth(): number {
    return nodeHeight(this.root);
  }

  static logarithmicDepthBound(length: number): number {
    return length === 0 ? 0 : 2 * Math.ceil(Math.log2(length + 1)) + 1;
  }

  has(value: string): boolean {
    return this.byValue.has(value);
  }

  at(rank: number, work = emptySequenceWork()): string | undefined {
    let current = this.root;
    let remaining = rank;
    while (current) {
      work.entriesExamined += 1;
      work.rankOrderComparisons += 1;
      const leftSize = nodeSize(current.left);
      if (remaining < leftSize) current = current.left;
      else if (remaining === leftSize) return current.value;
      else {
        remaining -= leftSize + 1;
        current = current.right;
      }
    }
    return undefined;
  }

  rankOf(value: string, work = emptySequenceWork()): number | undefined {
    let current: RankedNode | null = this.byValue.get(value) ?? null;
    if (!current) return undefined;
    let rank = nodeSize(current.left);
    work.entriesExamined += 1;
    while (current.parent) {
      const parent: RankedNode = current.parent;
      work.entriesExamined += 1;
      work.rankOrderComparisons += 1;
      if (parent.right === current) rank += nodeSize(parent.left) + 1;
      current = parent;
    }
    return rank;
  }

  insert(rank: number, value: string, work = emptySequenceWork()): void {
    if (rank < 0 || rank > this.length) throw new RangeError("sequence insertion rank out of bounds");
    if (this.byValue.has(value)) throw new Error("duplicate sequence value " + value);
    const node = new RankedNode(value);
    this.byValue.set(value, node);
    work.sequenceNodesAllocated += 1;
    work.allocatedBytes += 80;
    work.entriesMovedOrShifted += 1;
    const [left, right] = split(this.root, rank, work);
    this.root = joinWithPivot(left, node, right, work);
    setParent(this.root, null);
    recordDepth(this.root, work);
  }

  private detachSingleByValue(value: string, work: SequenceWork): [number, RankedNode] | undefined {
    let node = this.byValue.get(value);
    if (!node) return undefined;
    const rank = this.rankOf(value, work);
    if (rank === undefined) throw new Error("sequence rank map is inconsistent");

    let removal = node;
    if (node.left && node.right) {
      let replacement = node.right;
      while (replacement.left) {
        work.entriesExamined += 1;
        work.rankOrderComparisons += 1;
        replacement = replacement.left;
      }
      const replacementValue = replacement.value;
      node.value = replacementValue;
      replacement.value = value;
      this.byValue.set(replacementValue, node);
      this.byValue.set(value, replacement);
      work.entriesMovedOrShifted += 2;
      removal = replacement;
    }

    const parent = removal.parent;
    const child = removal.left ?? removal.right;
    if (!parent) {
      this.root = child;
    } else if (parent.left === removal) {
      parent.left = child;
    } else if (parent.right === removal) {
      parent.right = child;
    } else {
      throw new Error("sequence parent link is inconsistent");
    }
    setParent(child, parent);
    removal.left = null;
    removal.right = null;
    removal.parent = null;
    removal.size = 1;
    removal.height = 1;
    work.entriesMovedOrShifted += 1;

    let current = parent;
    while (current) {
      work.entriesExamined += 1;
      const ancestor = current.parent;
      const wasLeft = ancestor?.left === current;
      const balanced = rebalance(current, work);
      balanced.parent = ancestor;
      if (!ancestor) {
        this.root = balanced;
      } else if (wasLeft) {
        ancestor.left = balanced;
      } else {
        ancestor.right = balanced;
      }
      current = ancestor;
    }
    recordDepth(this.root, work);
    return [rank, removal];
  }

  remove(value: string, work = emptySequenceWork()): number | undefined {
    const detached = this.detachSingleByValue(value, work);
    if (!detached) return undefined;
    const [rank] = detached;
    this.byValue.delete(value);
    return rank;
  }

  extractByValue(value: string, count: number, work = emptySequenceWork()): SequenceFragment {
    if (count === 1) {
      const detached = this.detachSingleByValue(value, work);
      if (!detached) throw new Error("sequence value " + value + " is missing");
      return detached[1];
    }
    const rank = this.rankOf(value, work);
    if (rank === undefined) throw new Error("sequence value " + value + " is missing");
    return this.extract(rank, count, work);
  }

  extract(rank: number, count: number, work = emptySequenceWork()): SequenceFragment {
    if (rank < 0 || count < 0 || rank + count > this.length) throw new RangeError("sequence extraction range out of bounds");
    const [left, tail] = split(this.root, rank, work);
    const [fragment, right] = split(tail, count, work);
    this.root = join(left, right, work);
    setParent(this.root, null);
    setParent(fragment, null);
    work.entriesMovedOrShifted += count;
    recordDepth(this.root, work);
    recordDepth(fragment, work);
    return fragment;
  }

  extractRanks(
    ranks: readonly number[],
    work = emptySequenceWork(),
    deleteValues = false,
    expectedValues?: readonly string[],
  ): SequenceFragment[] {
    if (ranks.length === 0) return [];
    let previous = -1;
    const first = ranks[0];
    const span = ranks[ranks.length - 1] - first;
    let interpolatedRanks = ranks.length > 1;
    for (let index = 0; index < ranks.length; index += 1) {
      const rank = ranks[index];
      work.entriesExamined += 1;
      work.rankOrderComparisons += 1;
      if (!Number.isInteger(rank) || rank <= previous || rank >= this.length) {
        throw new RangeError("sequence batch ranks must be sorted, unique, and in bounds");
      }
      if (interpolatedRanks && rank !== first + Math.floor(index * span / (ranks.length - 1))) {
        interpolatedRanks = false;
      }
      previous = rank;
    }
    const removed: RankedNode[] = [];
    work.allocatedBytes += ranks.length * 8;
    this.root = extractRankedNodes(this.root, ranks, 0, ranks.length, 0, removed, work, interpolatedRanks);
    setParent(this.root, null);
    if (removed.length !== ranks.length) throw new Error("sequence batch extraction mismatch");
    if (expectedValues) {
      if (expectedValues.length !== removed.length) throw new Error("sequence batch expected-value count mismatch");
      for (let index = 0; index < removed.length; index += 1) {
        work.entriesExamined += 1;
        if (removed[index].value !== expectedValues[index]) throw new Error("sequence batch value mismatch");
      }
    }
    for (const node of removed) {
      if (deleteValues) this.byValue.delete(node.value);
      node.left = null;
      node.right = null;
      node.parent = null;
      node.size = 1;
      node.height = 1;
    }
    work.entriesMovedOrShifted += removed.length;
    recordDepth(this.root, work);
    return removed;
  }

  static combineSingletonFragments(
    fragments: readonly SequenceFragment[],
    work = emptySequenceWork(),
  ): SequenceFragment {
    work.entriesExamined += fragments.length;
    work.entriesMovedOrShifted += fragments.length;
    const root = buildBalancedFragments(fragments, 0, fragments.length, null);
    recordDepth(root, work);
    return root;
  }

  insertFragment(rank: number, fragment: SequenceFragment, work = emptySequenceWork(), adopt = false): void {
    if (!fragment) return;
    if (rank < 0 || rank > this.length) throw new RangeError("sequence fragment rank out of bounds");
    if (adopt) registerFragment(fragment, this.byValue, work);
    const [left, right] = split(this.root, rank, work);
    this.root = join(join(left, fragment, work), right, work);
    setParent(this.root, null);
    work.entriesMovedOrShifted += nodeSize(fragment);
    recordDepth(this.root, work);
  }

  deleteExtractedValue(value: string): void {
    this.byValue.delete(value);
  }

  slice(start: number, end: number, work = emptySequenceWork()): string[] {
    const boundedStart = Math.max(0, Math.min(this.length, start));
    const boundedEnd = Math.max(boundedStart, Math.min(this.length, end));
    const values: string[] = [];
    collectRange(this.root, boundedStart, boundedEnd, 0, values, work);
    work.entriesCopied += values.length;
    return values;
  }

  toArray(work = emptySequenceWork()): string[] {
    work.fullSequenceScans += 1;
    work.fullSequenceCopies += 1;
    return this.slice(0, this.length, work);
  }
}

function buildBalanced(
  values: string[],
  start: number,
  end: number,
  parent: RankedNode | null,
  byValue: Map<string, RankedNode>,
): RankedNode | null {
  if (start >= end) return null;
  const middle = start + Math.floor((end - start) / 2);
  const value = values[middle];
  if (byValue.has(value)) throw new Error("duplicate sequence value " + value);
  const node = new RankedNode(value);
  byValue.set(value, node);
  node.parent = parent;
  node.left = buildBalanced(values, start, middle, node, byValue);
  node.right = buildBalanced(values, middle + 1, end, node, byValue);
  update(node);
  return node;
}

function nodeSize(node: RankedNode | null): number {
  return node?.size ?? 0;
}

function nodeHeight(node: RankedNode | null): number {
  return node?.height ?? 0;
}

function update(node: RankedNode): void {
  node.size = 1 + nodeSize(node.left) + nodeSize(node.right);
  node.height = 1 + Math.max(nodeHeight(node.left), nodeHeight(node.right));
}

function setParent(node: RankedNode | null, parent: RankedNode | null): void {
  if (node) node.parent = parent;
}

function balanceFactor(node: RankedNode): number {
  return nodeHeight(node.left) - nodeHeight(node.right);
}

function rotateLeft(root: RankedNode, work: SequenceWork): RankedNode {
  const pivot = root.right;
  if (!pivot) throw new Error("AVL left rotation requires a right child");
  const transfer = pivot.left;
  root.right = transfer;
  setParent(transfer, root);
  pivot.left = root;
  root.parent = pivot;
  update(root);
  update(pivot);
  pivot.parent = null;
  work.treeRebalances += 1;
  return pivot;
}

function rotateRight(root: RankedNode, work: SequenceWork): RankedNode {
  const pivot = root.left;
  if (!pivot) throw new Error("AVL right rotation requires a left child");
  const transfer = pivot.right;
  root.left = transfer;
  setParent(transfer, root);
  pivot.right = root;
  root.parent = pivot;
  update(root);
  update(pivot);
  pivot.parent = null;
  work.treeRebalances += 1;
  return pivot;
}

function rebalance(root: RankedNode, work: SequenceWork): RankedNode {
  update(root);
  const balance = balanceFactor(root);
  if (balance > 1) {
    if (root.left && balanceFactor(root.left) < 0) {
      root.left = rotateLeft(root.left, work);
      setParent(root.left, root);
    }
    return rotateRight(root, work);
  }
  if (balance < -1) {
    if (root.right && balanceFactor(root.right) > 0) {
      root.right = rotateRight(root.right, work);
      setParent(root.right, root);
    }
    return rotateLeft(root, work);
  }
  root.parent = null;
  return root;
}

function joinWithPivot(
  left: RankedNode | null,
  pivot: RankedNode,
  right: RankedNode | null,
  work: SequenceWork,
): RankedNode {
  work.entriesExamined += 1;
  work.rankOrderComparisons += 1;
  if (nodeHeight(left) > nodeHeight(right) + 1) {
    if (!left) throw new Error("AVL join invariant failed");
    left.right = joinWithPivot(left.right, pivot, right, work);
    setParent(left.right, left);
    return rebalance(left, work);
  }
  if (nodeHeight(right) > nodeHeight(left) + 1) {
    if (!right) throw new Error("AVL join invariant failed");
    right.left = joinWithPivot(left, pivot, right.left, work);
    setParent(right.left, right);
    return rebalance(right, work);
  }
  pivot.left = left;
  pivot.right = right;
  setParent(left, pivot);
  setParent(right, pivot);
  update(pivot);
  pivot.parent = null;
  return pivot;
}

function removeMinimum(root: RankedNode, work: SequenceWork): [RankedNode | null, RankedNode] {
  work.entriesExamined += 1;
  work.rankOrderComparisons += 1;
  if (!root.left) {
    const remainder = root.right;
    setParent(remainder, null);
    root.right = null;
    root.parent = null;
    update(root);
    return [remainder, root];
  }
  const [left, minimum] = removeMinimum(root.left, work);
  root.left = left;
  setParent(left, root);
  return [rebalance(root, work), minimum];
}

function join(left: RankedNode | null, right: RankedNode | null, work: SequenceWork): RankedNode | null {
  if (!left || !right) {
    const result = left ?? right;
    setParent(result, null);
    return result;
  }
  const [remaining, pivot] = removeMinimum(right, work);
  return joinWithPivot(left, pivot, remaining, work);
}

function split(root: RankedNode | null, leftCount: number, work: SequenceWork): [RankedNode | null, RankedNode | null] {
  if (!root) return [null, null];
  work.entriesExamined += 1;
  work.rankOrderComparisons += 1;
  const leftTree = root.left;
  const rightTree = root.right;
  const leftSize = nodeSize(leftTree);
  root.left = null;
  root.right = null;
  root.parent = null;
  update(root);
  if (leftCount <= leftSize) {
    const [head, middle] = split(leftTree, leftCount, work);
    const tail = joinWithPivot(middle, root, rightTree, work);
    setParent(head, null);
    setParent(tail, null);
    return [head, tail];
  }
  const [middle, tail] = split(rightTree, leftCount - leftSize - 1, work);
  const head = joinWithPivot(leftTree, root, middle, work);
  setParent(head, null);
  setParent(tail, null);
  return [head, tail];
}

function extractRankedNodes(
  root: RankedNode | null,
  ranks: readonly number[],
  start: number,
  end: number,
  base: number,
  removed: RankedNode[],
  work: SequenceWork,
  interpolatedRanks: boolean,
): RankedNode | null {
  if (!root || start >= end) return root;
  work.entriesExamined += 1;
  work.rankOrderComparisons += 1;
  const leftTree = root.left;
  const rightTree = root.right;
  const nodeRank = base + nodeSize(leftTree);
  const selectedIndex = lowerBound(ranks, nodeRank, start, end, work, interpolatedRanks);
  const selected = selectedIndex < end && ranks[selectedIndex] === nodeRank;
  const rightStart = selected ? selectedIndex + 1 : selectedIndex;
  const left = extractRankedNodes(leftTree, ranks, start, selectedIndex, base, removed, work, interpolatedRanks);
  if (selected) removed.push(root);
  const right = extractRankedNodes(rightTree, ranks, rightStart, end, nodeRank + 1, removed, work, interpolatedRanks);
  root.left = null;
  root.right = null;
  root.parent = null;
  update(root);
  return selected ? join(left, right, work) : reattachOrJoin(left, root, right, work);
}

function lowerBound(
  values: readonly number[],
  target: number,
  start: number,
  end: number,
  work: SequenceWork,
  interpolatedRanks: boolean,
): number {
  let low = start;
  if (interpolatedRanks) {
    const first = values[0];
    const span = values[values.length - 1] - first;
    const candidate = Math.ceil((target - first) * (values.length - 1) / span);
    return Math.max(start, Math.min(end, candidate));
  }
  let high = end;
  while (low < high) {
    work.rankOrderComparisons += 1;
    const middle = low + Math.floor((high - low) / 2);
    if (values[middle] < target) low = middle + 1;
    else high = middle;
  }
  return low;
}

function reattachOrJoin(
  left: RankedNode | null,
  pivot: RankedNode,
  right: RankedNode | null,
  work: SequenceWork,
): RankedNode {
  if (Math.abs(nodeHeight(left) - nodeHeight(right)) <= 1) {
    pivot.left = left;
    pivot.right = right;
    setParent(left, pivot);
    setParent(right, pivot);
    update(pivot);
    pivot.parent = null;
    return pivot;
  }
  return joinWithPivot(left, pivot, right, work);
}

function buildBalancedFragments(
  fragments: readonly SequenceFragment[],
  start: number,
  end: number,
  parent: RankedNode | null,
): RankedNode | null {
  if (start >= end) return null;
  const middle = start + Math.floor((end - start) / 2);
  const node = fragments[middle];
  if (!node) throw new Error("cannot combine an empty sequence fragment");
  node.parent = parent;
  node.left = buildBalancedFragments(fragments, start, middle, node);
  node.right = buildBalancedFragments(fragments, middle + 1, end, node);
  update(node);
  return node;
}
function collectRange(
  node: RankedNode | null,
  start: number,
  end: number,
  base: number,
  output: string[],
  work: SequenceWork,
): void {
  if (!node || start >= end) return;
  work.entriesExamined += 1;
  const rank = base + nodeSize(node.left);
  if (start < rank) collectRange(node.left, start, end, base, output, work);
  if (start <= rank && rank < end) output.push(node.value);
  if (rank + 1 < end) collectRange(node.right, start, end, rank + 1, output, work);
}

function recordDepth(root: RankedNode | null, work: SequenceWork): void {
  work.maximumSequenceDepth = Math.max(work.maximumSequenceDepth, nodeHeight(root));
}

function registerFragment(node: RankedNode | null, byValue: Map<string, RankedNode>, work: SequenceWork): void {
  if (!node) return;
  const stack = [node];
  while (stack.length) {
    const current = stack.pop();
    if (!current) break;
    if (byValue.has(current.value)) throw new Error("duplicate sequence value " + current.value);
    byValue.set(current.value, current);
    work.entriesExamined += 1;
    if (current.left) stack.push(current.left);
    if (current.right) stack.push(current.right);
  }
}
