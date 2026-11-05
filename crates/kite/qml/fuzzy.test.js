// Run with: node --test crates/kite/qml/fuzzy.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/fuzzy.js', 'utf8').replace('.pragma library', '');
const F = new Function(src + '\nreturn {match, matchWord, markup, keycaps};')();

const order = (query, ...texts) => texts
  .map((t) => ({ t, m: F.match(query, t) })).filter((x) => x.m)
  .sort((a, b) => b.m.score - a.m.score).map((x) => x.t);

test('letters in order, not necessarily together', () => {
  assert.ok(F.match('nwt', 'New Tab'));
  assert.ok(F.match('NEW', 'new tab'));
  assert.strictEqual(F.match('tn', 'New Tab'), null);
  assert.strictEqual(F.match('xyz', 'New Tab'), null);
});

test('word starts and runs rank first', () => {
  assert.deepStrictEqual(order('new', 'Renew license', 'Open in New Tab', 'New Tab'), ['New Tab', 'Open in New Tab', 'Renew license']);
  assert.deepStrictEqual(order('nt', 'Internet', 'New Tab'), ['New Tab', 'Internet']);
  assert.deepStrictEqual(order('tab', 'Stable thing', 'Pin Tab'), ['Pin Tab', 'Stable thing']);
});

test('positions are the best ones, for highlighting', () => {
  // "New Tab": the word "New", not the n of nothing later.
  assert.deepStrictEqual(F.match('new', 'Renew New').positions, [6, 7, 8]);
  assert.deepStrictEqual(F.match('nt', 'New Tab').positions, [0, 4]);
});

test('several words match in any order', () => {
  assert.ok(F.match('tab new', 'New Tab'));
  assert.ok(F.match('privacy https', 'Privacy: HTTPS-only mode'));
  assert.strictEqual(F.match('new zebra', 'New Tab'), null);
  // Nothing typed: everything, in the order given.
  assert.deepStrictEqual(F.match('  ', 'New Tab'), { score: 0, positions: [] });
});

test('markup escapes, colours matches and dims the prefix', () => {
  assert.strictEqual(F.markup('a<b&', [], 'H', 'D', 0), 'a&lt;b&amp;');
  assert.strictEqual(F.markup('Tabs: New', [6], 'H', 'D', 6), '<font color="D">Tabs: </font><font color="H">N</font>ew');
  assert.ok(!F.markup('<script>"', [0], 'H', 'D', 0).includes('<script'));
});

test('keycaps split a shortcut, keeping a plus key', () => {
  assert.deepStrictEqual(F.keycaps('Ctrl+Shift+P'), ['Ctrl', 'Shift', 'P']);
  assert.deepStrictEqual(F.keycaps('Ctrl++'), ['Ctrl', '+']);
  assert.deepStrictEqual(F.keycaps('F12'), ['F12']);
});
