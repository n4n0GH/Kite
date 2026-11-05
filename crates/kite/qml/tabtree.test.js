// Run with: node --test crates/kite/qml/
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/tabtree.js', 'utf8').replace('.pragma library', '');
const T = new Function(src + '\nreturn {compute, parentAt, dropParent, ancestors, roots};')();

// [id, parent, extras]
const rows = list => list.map(([id, parent, more]) => Object.assign({id, parent, collapsed: false, pinned: false, pane: 0, group: ''}, more));
const field = (tree, k) => tree.map(t => t[k]);

test('depths, counts and links of a tree', () => {
  // 1
  //   2
  //     3
  //   4
  // 5
  const t = T.compute(rows([[1, -1], [2, 1], [3, 2], [4, 1], [5, -1]]));
  assert.deepStrictEqual(field(t, 'depth'), [0, 1, 2, 1, 0]);
  assert.deepStrictEqual(field(t, 'count'), [3, 1, 0, 0, 0]);
  assert.deepStrictEqual(field(t, 'parent'), [-1, 1, 2, 1, -1]);
  assert.deepStrictEqual(field(t, 'hidden'), [false, false, false, false, false]);
});

test('a collapsed tab hides all its descendants, and nothing else', () => {
  const t = T.compute(rows([[1, -1, {collapsed: true}], [2, 1], [3, 2], [4, 1], [5, -1]]));
  assert.deepStrictEqual(field(t, 'hidden'), [false, true, true, true, false]);
  assert.deepStrictEqual(field(t, 'count'), [3, 1, 0, 0, 0]);
});

test('a tab without descendants is not collapsed', () => {
  const t = T.compute(rows([[1, -1, {collapsed: true}], [2, -1]]));
  assert.deepStrictEqual(field(t, 'collapsed'), [false, false]);
});

test('links that cannot hold are dropped', () => {
  // The parent isn't above it / was separated from it by a root tab.
  assert.deepStrictEqual(field(T.compute(rows([[2, 1], [1, -1]])), 'parent'), [-1, -1]);
  assert.deepStrictEqual(field(T.compute(rows([[1, -1], [9, -1], [2, 1]])), 'parent'), [-1, -1, -1]);
  // Parent gone.
  assert.deepStrictEqual(field(T.compute(rows([[1, 7], [2, 1]])), 'parent'), [-1, 1]);
  // Other pane, other group, pinned (either side).
  assert.strictEqual(T.compute(rows([[1, -1], [2, 1, {pane: 1}]]))[1].parent, -1);
  assert.strictEqual(T.compute(rows([[1, -1, {group: 'g'}], [2, 1]]))[1].parent, -1);
  assert.strictEqual(T.compute(rows([[1, -1, {pinned: true}], [2, 1]]))[1].parent, -1);
  assert.strictEqual(T.compute(rows([[1, -1], [2, 1, {pinned: true}]]))[1].parent, -1);
});

test('the same group is fine', () => {
  assert.strictEqual(T.compute(rows([[1, -1, {group: 'g'}], [2, 1, {group: 'g'}]]))[1].parent, 1);
});

test('closing a parent: its children, linked to the grandparent, stay in place', () => {
  // 1 > 2 > (3, 4), 2 closed.
  const t = T.compute(rows([[1, -1], [3, 1], [4, 1]]));
  assert.deepStrictEqual(field(t, 'depth'), [0, 1, 1]);
});

test('a tab put in the middle of a tree keeps the tree intact', () => {
  const r = rows([[1, -1], [2, 1], [3, 1], [4, -1]]);
  assert.strictEqual(T.parentAt(r, 0), -1);
  assert.strictEqual(T.parentAt(r, 1), 1);
  assert.strictEqual(T.parentAt(r, 2), 1);
  assert.strictEqual(T.parentAt(r, 3), -1);
  assert.strictEqual(T.parentAt(r, 4), -1);
});

// The dragged tab (9, with `more` rows of descendants) already where it was dropped.
const drop = (list, wanted, more = 0) => {
  const r = rows(list);
  const first = r.findIndex(x => x.id === 9);
  return T.dropParent(r, first, first + more, wanted);
};

test('dropping: as deep as asked, between the limits', () => {
  // 1 > 2 > 3, dropped at the end.
  const list = [[1, -1], [2, 1], [3, 2], [9, -1]];
  assert.strictEqual(drop(list, 0), -1);
  assert.strictEqual(drop(list, 1), 1);
  assert.strictEqual(drop(list, 2), 2);
  assert.strictEqual(drop(list, 3), 3);
  assert.strictEqual(drop(list, 8), 3);    // at most a child of the tab above
  assert.strictEqual(drop(list, -2), -1);
});

test('dropping above a child: it keeps its parent', () => {
  // 1 > [9] > 2: the tab below is at depth 1, so 9 can't be a root tab there.
  assert.strictEqual(drop([[1, -1], [9, -1], [2, 1]], 0), 1);
  // ...and a child of 1 at most (2 stays 1's child).
  assert.strictEqual(drop([[1, -1], [9, -1], [2, 1]], 5), 1);
});

test('dropping first in the list', () => {
  assert.strictEqual(drop([[9, -1], [1, -1]], 3), -1);
});

test('dropping with descendants ignores them', () => {
  // 1, [9 > 8], 2
  const r = rows([[1, -1], [9, -1], [8, 9], [2, -1]]);
  assert.strictEqual(T.dropParent(r, 1, 2, 1), 1);
  assert.strictEqual(T.dropParent(r, 1, 2, 0), -1);
});

test('dropping after a collapsed tab goes beside it, never out of sight', () => {
  const c = {collapsed: true};
  assert.strictEqual(drop([[1, -1, c], [2, 1], [3, 2], [9, -1]], 5), -1);
  // Under a tab with a parent: beside it, so a child of that parent.
  assert.strictEqual(drop([[1, -1], [2, 1, c], [3, 2], [9, -1]], 5), 1);
});

test('nothing can be dropped under a pinned tab, nor can a pinned tab go under one', () => {
  assert.strictEqual(drop([[1, -1, {pinned: true}], [9, -1]], 1), -1);
  const r = rows([[1, -1], [9, -1, {pinned: true}]]);
  assert.strictEqual(T.dropParent(r, 1, 1, 1), -1);
});

test('dropping only nests among tabs of the same group', () => {
  const g = {group: 'g'};
  // Just outside a group, next to a nested tab of it: no parent there.
  assert.strictEqual(drop([[1, -1, g], [2, 1, g], [9, -1]], 2), -1);
  // Inside it: as usual.
  assert.strictEqual(drop([[1, -1, g], [2, 1, g], [9, -1, g]], 1), 1);
  assert.strictEqual(drop([[1, -1, g], [2, 1, g], [9, -1, g]], 2), 2);
  // At a group's end, the tab below being outside doesn't limit it.
  assert.strictEqual(drop([[1, -1], [2, 1], [3, -1, g], [9, -1, g], [4, 1, {group: 'h'}]], 0), -1);
  // Just above a group, the group's nested first tab doesn't limit it either.
  assert.strictEqual(drop([[1, -1], [9, -1], [2, -1, g], [3, 2, g]], 1), 1);
});

test('roots: the tabs not under another of the given tabs', () => {
  // 1 > 2 > 3, 1 > 4, 5
  const r = rows([[1, -1], [2, 1], [3, 2], [4, 1], [5, -1]]);
  assert.deepStrictEqual(T.roots(r, [1, 2, 3, 4, 5]), [1, 5]);
  assert.deepStrictEqual(T.roots(r, [2, 3]), [2]);
  assert.deepStrictEqual(T.roots(r, [3, 4]), [3, 4]);
  // An ancestor further up counts, with the one between not given.
  assert.deepStrictEqual(T.roots(r, [1, 3]), [1]);
});

test('ancestors, nearest first', () => {
  assert.deepStrictEqual(T.ancestors(rows([[1, -1], [2, 1], [3, 2]]), 3), [2, 1]);
  assert.deepStrictEqual(T.ancestors(rows([[1, -1]]), 1), []);
  assert.deepStrictEqual(T.ancestors(rows([[1, -1]]), 42), []);
});
