// Run with: node --test crates/kite/plugins/gestures/strokes.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/strokes.js', 'utf8').replace('.pragma library', '');
const S = new Function(src + '\nreturn {tracker, track, arrows, assignable, sorted, match};')();

const stroke = (...points) => {
  const [x, y] = points[0];
  const t = S.tracker(x, y);
  let out = '';
  for (const [px, py] of points.slice(1)) out = S.track(t, px, py);
  return out;
};
// A line from a to b, in small moves, as a mouse reports it.
const line = (a, b, n = 20) => Array.from({length: n + 1}, (_, i) => [a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n]);

test('straight lines', () => {
  assert.strictEqual(stroke(...line([200, 200], [100, 200])), 'L');
  assert.strictEqual(stroke(...line([200, 200], [300, 200])), 'R');
  assert.strictEqual(stroke(...line([200, 200], [200, 100])), 'U');
  assert.strictEqual(stroke(...line([200, 200], [200, 300])), 'D');
});

test('turns, and going back on itself', () => {
  assert.strictEqual(stroke(...line([200, 200], [200, 300]), ...line([200, 300], [300, 300])), 'DR');
  assert.strictEqual(stroke(...line([200, 200], [200, 100]), ...line([200, 100], [200, 200])), 'UD');
});

test('a hand is not a ruler', () => {
  const wobbly = line([200, 200], [400, 200]).map(([x, y], i) => [x, y + (i % 2 ? 6 : -6)]);
  assert.strictEqual(stroke(...wobbly), 'R');
  // A drift to one side along the way doesn't add a turn.
  assert.strictEqual(stroke(...line([200, 200], [400, 230])), 'R');
});

test('too short to be anything', () => {
  assert.strictEqual(stroke([200, 200], [210, 205], [215, 195]), '');
});

test('a clear diagonal is no direction', () => {
  assert.strictEqual(stroke(...line([200, 200], [300, 300])), '');
});

test('the same move in one report or many', () => {
  assert.strictEqual(stroke([200, 200], [200, 400]), 'D');
  assert.strictEqual(stroke([200, 200], [200, 400], [500, 400]), 'DR');
});

// A circle from the top, going left first (or right, mirrored), with some tremor.
const circle = (r, dir = -1, tremor = 0) => {
  const out = [];
  for (let i = 0; i <= 90; i++) {
    const a = i / 90 * 2 * Math.PI;
    const shake = tremor ? Math.sin(i * 2.7) * tremor : 0;
    out.push([400 + dir * (r + shake) * Math.sin(a), 400 - (r + shake) * Math.cos(a)]);
  }
  return out;
};

test('a recorded circle is found again, large or small, steady or not', () => {
  // Whichever way the recording itself came out.
  for (const recorded of [{LDRUL: 'reload'}, {LDRU: 'reload'}]) {
    for (const r of [70, 100, 150, 250]) {
      assert.ok(S.match(recorded, stroke(...circle(r))), 'r ' + r);
      assert.ok(S.match(recorded, stroke(...circle(r, -1, 3))), 'tremor, r ' + r);
      assert.strictEqual(S.match(recorded, stroke(...circle(r, 1))), '', 'the other way round is another gesture, r ' + r);
    }
  }
});

test('matching: exact first, one off only for long strokes, never a guess', () => {
  const map = {L: 1, DR: 1, UDU: 1, LDRUL: 1, RDLUR: 1};
  assert.strictEqual(S.match(map, 'L'), 'L');
  assert.strictEqual(S.match(map, 'LDRUL'), 'LDRUL');
  assert.strictEqual(S.match(map, 'LDRU'), 'LDRUL');
  assert.strictEqual(S.match(map, 'LDRULD'), 'LDRUL');
  assert.strictEqual(S.match(map, 'R'), '');       // short: exact only
  assert.strictEqual(S.match(map, 'DRU'), '');
  assert.strictEqual(S.match(map, 'UDUD'), '');    // a short stroke isn't near anything
  assert.strictEqual(S.match(map, 'LDR'), '');     // too far off
  assert.strictEqual(S.match({LDRUL: 1, LDRUR: 1}, 'LDRU'), '');   // could be either
  assert.strictEqual(S.match({LDRUL: 1, LDRU: 1}, 'LDRU'), 'LDRU');
  assert.strictEqual(S.match({}, 'constructor'), '');
  assert.strictEqual(S.match({}, ''), '');
});

test('what can be given an action', () => {
  for (const s of ['L', 'DR', 'LDRUL', 'LRLRLRLRLRLR']) assert.ok(S.assignable(s), s);
  for (const s of ['', 'X', 'LL', 'DRR', 'LRLRLRLRLRLRL', 'constructor']) assert.ok(!S.assignable(s), s);
});

test('short strokes are listed first', () => {
  assert.deepStrictEqual(S.sorted({LDRUL: 1, R: 1, DR: 1, L: 1, UD: 1}), ['L', 'R', 'DR', 'UD', 'LDRUL']);
});

test('arrows', () => {
  assert.strictEqual(S.arrows('DR'), '↓→');
  assert.strictEqual(S.arrows(''), '');
});

test('the default gestures are possible, and name real actions', () => {
  const plugin = JSON.parse(fs.readFileSync(__dirname + '/plugin.json', 'utf8'));
  const main = fs.readFileSync(__dirname + '/../../qml/Main.qml', 'utf8');
  const actions = new Set([...main.matchAll(/\{id: "(\w+)", group: "/g)].map((m) => m[1]));
  for (const [s, action] of Object.entries(plugin.settings.map)) {
    assert.ok(S.assignable(s), s);
    assert.ok(actions.has(action), action);
  }
});
