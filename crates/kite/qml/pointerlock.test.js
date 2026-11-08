// Run with: node --test crates/kite/qml/pointerlock.test.js
// Pointer lock's page script (js/pointerlock.js) and its isolated-world relay
// (js/pointerlock_relay.js), against a stand-in for a page, with Kite's side
// played by the test.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const pageSrc = fs.readFileSync(__dirname + '/../src/js/pointerlock.js', 'utf8');
const relaySrc = fs.readFileSync(__dirname + '/../src/js/pointerlock_relay.js', 'utf8').replace('__KITE_FRAME_SECRET__', 'S3CRET');

const tick = () => new Promise((r) => setTimeout(r, 1));

function page({ activated = true } = {}) {
  class Element extends EventTarget {
    constructor(root) { super(); this.root = root; this.isConnected = true; }
    getRootNode() { return this.root; }
  }
  Element.prototype.requestPointerLock = function () { throw new Error('native'); };
  class Document extends EventTarget {}
  class ShadowRoot extends EventTarget {}
  class MouseEvent extends Event {
    constructor(type, init) {
      super(type, init);
      const { bubbles, cancelable, composed, ...rest } = init;
      Object.assign(this, rest);
    }
  }
  class PointerEvent extends MouseEvent {}
  const document = new Document();
  document.hidden = false;
  const window = new EventTarget();
  const up = [];
  const userActivation = { isActive: activated };
  const globals = {
    Element, Document, ShadowRoot, MouseEvent, PointerEvent, document, window,
    addEventListener: window.addEventListener.bind(window),
    console: { debug: (m) => up.push(m) },
    navigator: { userActivation },
  };
  const events = [];
  for (const t of ['pointerlockchange', 'pointerlockerror']) document.addEventListener(t, () => events.push(t));
  new Function(...Object.keys(globals), pageSrc)(...Object.values(globals));
  new Function(...Object.keys(globals), relaySrc)(...Object.values(globals));
  const kite = (m, a) => globalThis.__kitePlDown(m, a);
  const sent = () => up.splice(0).map((m) => {
    assert.ok(m.startsWith('S3CRETpl:'));
    return JSON.parse(m.slice('S3CRETpl:'.length));
  });
  const el = new Element(document);
  const moves = [];
  for (const t of ['pointermove', 'mousemove']) el.addEventListener(t, (e) => moves.push([t, e.movementX, e.movementY]));
  return { document, window, el, kite, sent, events, moves, userActivation, Element, MouseEvent };
}

test('a page can lock the pointer only on a click', async () => {
  const p = page({ activated: false });
  await assert.rejects(p.el.requestPointerLock(), { name: 'NotAllowedError' });
  assert.deepStrictEqual(p.sent(), []);
  assert.deepStrictEqual(p.events, ['pointerlockerror']);
  assert.strictEqual(p.document.pointerLockElement, null);
});

test('locking, moving and letting go', async () => {
  const p = page();
  const asked = p.el.requestPointerLock();
  const [lock] = p.sent();
  assert.strictEqual(lock.m, 'lock');
  assert.strictEqual(lock.a.unadjusted, false);
  p.kite('answer', { id: lock.a.id, error: '' });
  await asked;
  assert.strictEqual(p.document.pointerLockElement, p.el);
  assert.deepStrictEqual(p.events, ['pointerlockchange']);

  // Whole pixels go out; what's left over is added to the next movement.
  p.kite('move', [1.5, -2.25]);
  p.kite('move', [0.5, 0.25]);
  p.kite('move', [0.25, 0]);
  assert.deepStrictEqual(p.moves, [['pointermove', 1, -2], ['mousemove', 1, -2], ['pointermove', 1, 0], ['mousemove', 1, 0]]);

  // Esc and the like: Kite says it's lost.
  p.kite('lost', {});
  assert.strictEqual(p.document.pointerLockElement, null);
  assert.deepStrictEqual(p.events, ['pointerlockchange', 'pointerlockchange']);
  // Movement still on its way tells Kite to let go.
  p.kite('move', [3, 3]);
  assert.deepStrictEqual(p.sent().map((m) => m.m), ['unlock']);
  assert.strictEqual(p.moves.length, 4);
});

test('the page letting go, and locking again without a click', async () => {
  const p = page();
  const asked = p.el.requestPointerLock({ unadjustedMovement: true });
  const [lock] = p.sent();
  assert.strictEqual(lock.a.unadjusted, true);
  p.kite('answer', { id: lock.a.id, error: '' });
  await asked;
  p.document.exitPointerLock();
  assert.deepStrictEqual(p.sent().map((m) => m.m), ['unlock']);
  // (The change comes after the call.)
  assert.strictEqual(p.document.pointerLockElement, p.el);
  await tick();
  assert.strictEqual(p.document.pointerLockElement, null);
  p.kite('lost', {});
  assert.deepStrictEqual(p.events, ['pointerlockchange', 'pointerlockchange']);

  p.userActivation.isActive = false;
  const again = p.el.requestPointerLock();
  const [relock] = p.sent();
  assert.strictEqual(relock.m, 'lock');
  p.kite('answer', { id: relock.a.id, error: '' });
  await again;
  assert.strictEqual(p.document.pointerLockElement, p.el);
});

test("Kite's refusal reaches the page", async () => {
  const p = page();
  const asked = p.el.requestPointerLock();
  const [lock] = p.sent();
  p.kite('answer', { id: lock.a.id, error: 'Turned off.' });
  await assert.rejects(asked, { name: 'NotAllowedError', message: 'Turned off.' });
  assert.deepStrictEqual(p.events, ['pointerlockerror']);
  assert.strictEqual(p.document.pointerLockElement, null);
  // An answer for no request does nothing.
  p.kite('answer', { id: lock.a.id, error: '' });
  assert.strictEqual(p.document.pointerLockElement, null);
});

test('a removed element loses the lock', async () => {
  const p = page();
  const asked = p.el.requestPointerLock();
  const [lock] = p.sent();
  p.kite('answer', { id: lock.a.id, error: '' });
  await asked;
  p.el.isConnected = false;
  p.kite('move', [5, 5]);
  assert.deepStrictEqual(p.moves, []);
  assert.deepStrictEqual(p.sent().map((m) => m.m), ['unlock']);
  await tick();
  assert.strictEqual(p.document.pointerLockElement, null);
});

test('requests for elements not in the page fail', async () => {
  const p = page();
  const loose = new p.Element(null);
  loose.isConnected = false;
  await assert.rejects(loose.requestPointerLock(), { name: 'WrongDocumentError' });
  assert.deepStrictEqual(p.sent(), []);
});

test("the browser's own movement doesn't reach the page while locked", async () => {
  const p = page();
  const seen = [];
  p.window.addEventListener('mousemove', (e) => seen.push(e.movementY), { capture: true });
  const real = (dy) => {
    const e = new p.MouseEvent('mousemove', { clientX: 10, clientY: 10, movementX: 0, movementY: dy, buttons: 0 });
    Object.defineProperty(e, 'isTrusted', { value: true });
    p.window.dispatchEvent(e);
  };
  real(-1);
  const asked = p.el.requestPointerLock();
  const [lock] = p.sent();
  p.kite('answer', { id: lock.a.id, error: '' });
  await asked;
  real(-1);
  real(-1);
  p.kite('move', [0, 4]);
  assert.deepStrictEqual(p.moves, [['pointermove', 0, 4], ['mousemove', 0, 4]]);
  p.kite('lost', {});
  real(-2);
  assert.deepStrictEqual(seen, [-1, -2]);
});
