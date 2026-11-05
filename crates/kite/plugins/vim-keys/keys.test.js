// Run with: node --test crates/kite/plugins/vim-keys/keys.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/keys.js', 'utf8').replace('.pragma library', '');
const K = new Function(src + '\nreturn {COMMANDS, WINDOW_ACTIONS, bindings, press, conflicts, labels, siteOf, isExcepted, hintTheme, DEFAULT_HINT_COLOR, scrollSpeed, SCROLL_SPEEDS};')();
const plugin = JSON.parse(fs.readFileSync(__dirname + '/plugin.json', 'utf8'));
const map = K.bindings(plugin.settings.keys);

test('single keys, and those that are the page\'s', () => {
  assert.deepStrictEqual(K.press(map, '', 'j'), {run: 'scrollDown'});
  assert.deepStrictEqual(K.press(map, '', 'G'), {run: 'bottom'});
  assert.deepStrictEqual(K.press(map, '', 'q'), {});
  assert.deepStrictEqual(K.press(map, '', 'Q'), {});
});

test('sequences', () => {
  assert.deepStrictEqual(K.press(map, '', 'g'), {pending: 'g'});
  assert.deepStrictEqual(K.press(map, 'g', 'g'), {run: 'top'});
  assert.deepStrictEqual(K.press(map, '', 'y'), {pending: 'y'});
  assert.deepStrictEqual(K.press(map, 'y', 'y'), {run: 'copyUrl'});
});

test('a wrong second key starts over with itself', () => {
  assert.deepStrictEqual(K.press(map, 'g', 'j'), {run: 'scrollDown'});
  assert.deepStrictEqual(K.press(map, 'g', 'q'), {});
  assert.deepStrictEqual(K.press(map, 'g', 'y'), {pending: 'y'});
});

test('names of the object itself are no bindings', () => {
  assert.deepStrictEqual(K.press({}, '', 'constructor'), {});
});

test('the defaults are complete and can all work', () => {
  assert.deepStrictEqual(K.conflicts(plugin.settings.keys), []);
  assert.deepStrictEqual(K.COMMANDS.map((c) => c.id).sort(), Object.keys(plugin.settings.keys).sort());
});

test('window commands are real actions', () => {
  const main = fs.readFileSync(__dirname + '/../../qml/Main.qml', 'utf8');
  const actions = new Set([...main.matchAll(/\{id: "(\w+)", group: "/g)].map((m) => m[1]));
  for (const a of K.WINDOW_ACTIONS) assert.ok(actions.has(a), a);
  for (const a of K.WINDOW_ACTIONS) assert.ok(K.COMMANDS.some((c) => c.id === a), a);
});

test('conflicts: the same keys, and one starting another', () => {
  const keys = (o) => Object.assign({}, plugin.settings.keys, o);
  assert.deepStrictEqual(K.conflicts(keys({scrollUp: 'j'})).sort(), ['scrollDown', 'scrollUp']);
  assert.deepStrictEqual(K.conflicts(keys({reload: 'g'})).sort(), ['reload', 'top']);
  assert.deepStrictEqual(K.conflicts(keys({reload: 'gr'})), []);
  assert.deepStrictEqual(K.conflicts(keys({reload: ''})), []);
});

test('an unbound command is simply not there', () => {
  const m = K.bindings(Object.assign({}, plugin.settings.keys, {scrollDown: ''}));
  assert.deepStrictEqual(K.press(m, '', 'j'), {});
});

test('hint labels: enough, different, none starting another, shortest possible', () => {
  const chars = plugin.settings.hintChars;
  for (const n of [1, 2, 13, 14, 15, 196, 197, 1000]) {
    const l = K.labels(n, chars);
    assert.strictEqual(l.length, n);
    assert.strictEqual(new Set(l).size, n);
    for (const a of l) for (const b of l) if (a !== b) assert.ok(!a.startsWith(b), a + ' ' + b);
    assert.strictEqual(l[0].length, n <= chars.length ? 1 : n <= chars.length ** 2 ? 2 : 3, String(n));
  }
  assert.deepStrictEqual(K.labels(3, 'ab').map((x) => x.length), [2, 2, 2]);
});

test('hint labels from a poor set of letters', () => {
  assert.deepStrictEqual(K.labels(5, ''), []);
  assert.deepStrictEqual(K.labels(5, 'aaaa'), []);
  assert.deepStrictEqual(K.labels(0, 'abc'), []);
  assert.strictEqual(K.labels(4, 'aA b').length, 4);
});

test('moving tabs: two keys of the same, reachable', () => {
  assert.deepStrictEqual(K.press(map, '', '<'), {pending: '<'});
  assert.deepStrictEqual(K.press(map, '<', '<'), {run: 'moveTabLeft'});
  assert.deepStrictEqual(K.press(map, '>', '>'), {run: 'moveTabRight'});
});

const luminance = (hex) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.substring(i, i + 2), 16));
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
};

test('hint colors: a gradient going down to it, a darker outline, readable text', () => {
  for (const c of ['#ffc542', '#3daee9', '#da4453', '#000000', '#ffffff', '#27ae60']) {
    const t = K.hintTheme(c);
    assert.strictEqual(t.bottom, c);
    assert.ok(luminance(t.top) >= luminance(t.bottom), c);
    assert.ok(luminance(t.outline) <= luminance(t.bottom), c);
    assert.ok(Math.abs(luminance(t.text) - luminance(t.bottom)) > 0.4, c + ' text ' + t.text);
    for (const v of Object.values(t)) assert.match(v, /^#[0-9a-f]{6}$/);
  }
});

test('hint colors: only colors reach the style sheet', () => {
  const fallback = K.hintTheme(K.DEFAULT_HINT_COLOR);
  for (const bad of ['', 'red', '#12', '#ffc54}', '#ffc542;}*{display:none', null, undefined]) {
    assert.deepStrictEqual(K.hintTheme(bad), fallback, String(bad));
  }
});

test('the default hint color is the one in the settings', () => {
  assert.strictEqual(plugin.settings.hintColor, K.DEFAULT_HINT_COLOR);
});

test('scroll speed: what is set, within limits, else the default', () => {
  assert.strictEqual(K.scrollSpeed(800), 800);
  assert.strictEqual(K.scrollSpeed(1), K.SCROLL_SPEEDS.least);
  assert.strictEqual(K.scrollSpeed(1e9), K.SCROLL_SPEEDS.most);
  for (const bad of [0, -5, 'fast', null, undefined, NaN, Infinity]) assert.strictEqual(K.scrollSpeed(bad), K.SCROLL_SPEEDS.normal, String(bad));
  assert.strictEqual(plugin.settings.scrollSpeed, K.SCROLL_SPEEDS.normal);
});

test('sites left alone', () => {
  assert.strictEqual(K.siteOf('https://www.Example.com:8/a'), 'example.com');
  assert.strictEqual(K.siteOf('kite:start'), '');
  assert.ok(K.isExcepted('mail.example.com', ['example.com']));
  assert.ok(!K.isExcepted('notexample.com', ['example.com']));
});
