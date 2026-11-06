// Run with: node --test crates/kite/qml/third_party_storage.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/../src/js/third_party_storage.js', 'utf8');

// A frame's window, refused storage or not.
function frame({ top = false, origin = 'https://player.example', refused = true, error = 'SecurityError' } = {}) {
  const w = { origin };
  w.top = top ? w : {};
  w.Storage = class Storage {};
  for (const name of ['localStorage', 'sessionStorage']) {
    Object.defineProperty(w, name, {
      configurable: true,
      get() { if (refused) { const e = new Error('denied'); e.name = error; throw e; } return 'real ' + name; },
    });
  }
  new Function('globalThis', src)(w);
  return w;
}

test('a refused frame gets storage that works', () => {
  const { localStorage: s } = frame();
  assert.strictEqual(s.getItem('a'), null);
  s.setItem('a', 1);
  assert.strictEqual(s.getItem('a'), '1');
  assert.strictEqual(s.length, 1);
  assert.strictEqual(s.key(0), 'a');
  assert.strictEqual(s.key(5), null);
  s.removeItem('a');
  assert.strictEqual(s.length, 0);
  s.setItem('b', 'x'); s.clear();
  assert.strictEqual(s.getItem('b'), null);
});

test('as properties too, as much code uses it', () => {
  const { localStorage: s } = frame();
  s.volume = 0.5;
  assert.strictEqual(s.volume, '0.5');
  assert.strictEqual(s.getItem('volume'), '0.5');
  assert.ok('volume' in s);
  assert.deepStrictEqual(Object.keys(s), ['volume']);
  assert.strictEqual(JSON.stringify(s), '{"volume":"0.5"}');
  delete s.volume;
  assert.strictEqual(s.volume, null);
  // Its own methods can't be overwritten by data.
  s.setItem('getItem', 'x');
  assert.strictEqual(typeof s.getItem, 'function');
});

test('nothing is shared', () => {
  const w = frame();
  w.localStorage.setItem('a', '1');
  assert.strictEqual(w.sessionStorage.getItem('a'), null, 'between its two');
  assert.strictEqual(frame().localStorage.getItem('a'), null, 'with another frame');
});

test('looks like storage', () => {
  const w = frame();
  assert.ok(w.localStorage instanceof w.Storage);
});

test('only where storage was refused', () => {
  assert.strictEqual(frame({ refused: false }).localStorage, 'real localStorage');
  assert.throws(() => frame({ top: true }).localStorage, /denied/, 'not the top page: that takes "block all cookies"');
  assert.throws(() => frame({ origin: 'null' }).localStorage, /denied/, 'not sandboxed frames');
  assert.throws(() => frame({ error: 'OtherError' }).localStorage, /denied/, 'not other errors');
});
