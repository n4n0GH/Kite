// Run with: node --test crates/kite/qml/webmidi.test.js
// The page's side of Web MIDI, with what's on the other side of the bridge played by the test.
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/../src/js/webmidi.js', 'utf8');

const INPUT = { id: 'in1', name: 'Synth In', type: 'input' };
const OUTPUT = { id: 'out1', name: 'Synth Out', type: 'output' };

// A page. `answer(request)` says what comes back for each request; ops collects the rest.
function page({ answer = () => ({ granted: true, ports: [INPUT, OUTPUT] }), state = 'prompt', secure = true, subframe = false } = {}) {
  class Navigator { requestMIDIAccess() { return 'native'; } }
  class Permissions { query(d) { return Promise.resolve({ native: d.name }); } }
  const document = new EventTarget();
  const windowEvents = new EventTarget();
  const sandbox = {
    EventTarget, Event, DOMException, setTimeout, clearTimeout, performance, CustomEvent,
    Navigator, Permissions, document,
    navigator: new Navigator(),
    isSecureContext: secure,
    addEventListener: windowEvents.addEventListener.bind(windowEvents),
  };
  sandbox.window = sandbox;
  sandbox.top = subframe ? {} : sandbox;
  vm.createContext(sandbox);
  const ops = [];
  const requests = [];
  const reply = (name, detail) => document.dispatchEvent(new CustomEvent(name, { detail: JSON.stringify(detail) }));
  document.addEventListener('kite-midi-request', (e) => {
    const r = JSON.parse(e.detail);
    requests.push(r);
    const a = answer(r);
    if (a) reply('kite-midi-answer', { id: r.id, ...a });
  });
  document.addEventListener('kite-midi-query', (e) => {
    const r = JSON.parse(e.detail);
    reply('kite-midi-state', { id: r.id, state });
  });
  document.addEventListener('kite-midi-op', (e) => ops.push(JSON.parse(e.detail)));
  vm.runInContext(source, sandbox);
  return {
    navigator: sandbox.navigator,
    window: sandbox,
    ops,
    requests,
    event: (detail) => reply('kite-midi-event', detail),
    leave: (persisted) => { const e = new Event('pagehide'); e.persisted = persisted; windowEvents.dispatchEvent(e); },
    permissions: new Permissions(),
    Permissions,
  };
}

const access = (p, options) => p.navigator.requestMIDIAccess(options);
const sends = (p) => p.ops.filter((o) => o.op === 'send');
const only = (map) => [...map.values()][0];

test('left alone where it cannot work', () => {
  assert.strictEqual(page({ secure: false }).navigator.requestMIDIAccess(), 'native');
  assert.strictEqual(page({ subframe: true }).navigator.requestMIDIAccess(), 'native');
});

test('access lists ports, and says if SysEx came with it', async () => {
  const p = page();
  const a = await access(p);
  assert.strictEqual(a.sysexEnabled, false);
  assert.strictEqual(a.inputs.size, 1);
  assert.strictEqual(a.outputs.size, 1);
  const input = a.inputs.get('in1');
  assert.strictEqual(input.name, 'Synth In');
  assert.strictEqual(input.type, 'input');
  assert.strictEqual(input.state, 'connected');
  assert.strictEqual(input.connection, 'closed');
  assert.strictEqual(Object.prototype.toString.call(a), '[object MIDIAccess]');
  assert.deepStrictEqual([...a.outputs.keys()], ['out1']);
  assert.strictEqual((await access(p, { sysex: true })).sysexEnabled, true);
  assert.deepStrictEqual(p.requests.map((r) => r.sysex), [false, true]);
});

test('refused, or without ALSA', async () => {
  await assert.rejects(access(page({ answer: () => ({ granted: false }) })), (e) => e.name === 'NotAllowedError');
  await assert.rejects(access(page({ answer: () => ({ granted: true, error: 'no ALSA' }) })), (e) => e.name === 'NotSupportedError');
});

test('waits for the answer', async () => {
  let done = false;
  const p = page({ answer: () => null });
  access(p).then(() => { done = true; });
  await new Promise((r) => setTimeout(r, 5));
  assert.strictEqual(done, false);
});

test('an input opens by listening, however that is done', async () => {
  for (const listen of [(i, f) => { i.onmidimessage = f; }, (i, f) => i.addEventListener('midimessage', f)]) {
    const p = page();
    const input = only((await access(p)).inputs);
    const got = [];
    listen(input, (e) => got.push([...e.data]));
    assert.strictEqual(input.connection, 'open');
    assert.deepStrictEqual(p.ops, [{ op: 'open', id: 'in1' }]);
    p.event({ event: 'message', port: 'in1', data: [0x90, 60, 100] });
    assert.deepStrictEqual(got, [[0x90, 60, 100]]);
  }
});

test('an input that is not open hears nothing, and closing ends it', async () => {
  const p = page();
  const input = only((await access(p)).inputs);
  const got = [];
  EventTarget.prototype.addEventListener.call(input, 'midimessage', (e) => got.push(e));
  p.event({ event: 'message', port: 'in1', data: [0x90, 60, 100] });
  assert.strictEqual(got.length, 0);
  await input.open();
  p.event({ event: 'message', port: 'in1', data: [0x90, 60, 100] });
  assert.strictEqual(got.length, 1);
  await input.close();
  assert.deepStrictEqual(p.ops, [{ op: 'open', id: 'in1' }, { op: 'close', id: 'in1' }]);
  p.event({ event: 'message', port: 'in1', data: [0x90, 60, 100] });
  assert.strictEqual(got.length, 1);
});

test('an input stays open while another access has it open', async () => {
  const p = page();
  const [one, two] = [only((await access(p)).inputs), only((await access(p)).inputs)];
  await one.open();
  await two.open();
  assert.deepStrictEqual(p.ops, [{ op: 'open', id: 'in1' }], 'once');
  await one.close();
  assert.strictEqual(p.ops.length, 1);
  await two.close();
  assert.deepStrictEqual(p.ops[1], { op: 'close', id: 'in1' });
});

test('an output connects to its device while open, as hardware needs, and lets go', async () => {
  const p = page();
  const output = only((await access(p)).outputs);
  await output.open();
  assert.strictEqual(output.connection, 'open');
  assert.deepStrictEqual(p.ops, [{ op: 'open', id: 'out1' }]);
  await output.close();
  assert.strictEqual(output.connection, 'closed');
  assert.deepStrictEqual(p.ops[1], { op: 'close', id: 'out1' });
  output.send([0x90, 1, 1]);
  assert.deepStrictEqual(p.ops.slice(2).map((o) => o.op), ['open', 'send'], 'sending opens it first');
});

test('SysEx is not delivered without having asked for it', async () => {
  const p = page();
  const input = only((await access(p)).inputs);
  const got = [];
  input.onmidimessage = (e) => got.push(e);
  p.event({ event: 'message', port: 'in1', data: [0xF0, 1, 0xF7] });
  assert.strictEqual(got.length, 0);
});

test('send: what the standard refuses', async () => {
  const p = page();
  const output = only((await access(p)).outputs);
  output.send([0x90, 60, 100]);
  output.send(Uint8Array.of(0xC0, 5));
  assert.deepStrictEqual(sends(p).map((o) => o.data), [[0x90, 60, 100], [0xC0, 5]]);
  assert.strictEqual(output.connection, 'open', 'sending opens it');
  for (const bad of [[60], [0x90, 60], [0x90, 60, 200], [0xF7], [0xF4], [300], [-1], ['x']]) {
    assert.throws(() => output.send(bad), (e) => e.name === 'TypeError', JSON.stringify(bad));
  }
  assert.throws(() => output.send([0xF0, 1, 0xF7]), (e) => e.name === 'InvalidAccessError');
  assert.throws(() => output.send([0x90, 1, 2, 0xF0, 1, 0xF7]), (e) => e.name === 'InvalidAccessError');
  assert.strictEqual(sends(p).length, 2, 'none of that was sent');
});

test('send: SysEx with access to it, and whole only', async () => {
  const p = page();
  const output = only((await access(p, { sysex: true })).outputs);
  output.send([0xF0, 1, 2, 0xF7]);
  output.send([0xF0, 1, 0xF8, 2, 0xF7]);
  assert.strictEqual(sends(p).length, 2);
  assert.throws(() => output.send([0xF0, 1, 2]), (e) => e.name === 'TypeError');
  assert.throws(() => output.send([0xF0, 1, 0x90, 0xF7]), (e) => e.name === 'TypeError');
});

test('send later, and clear what has not gone yet', async () => {
  const p = page();
  const output = only((await access(p)).outputs);
  const at = performance.now();
  output.send([0x90, 1, 1], at + 20);
  output.send([0x90, 2, 2], at + 400);
  assert.strictEqual(sends(p).length, 0);
  await new Promise((r) => setTimeout(r, 60));
  assert.deepStrictEqual(sends(p).map((o) => o.data), [[0x90, 1, 1]]);
  output.clear();
  await new Promise((r) => setTimeout(r, 420));
  assert.strictEqual(sends(p).length, 1);
  output.send([0x90, 3, 3], at);
  assert.strictEqual(sends(p).length, 2, 'a time gone by is now');
});

test('devices coming and going', async () => {
  const p = page();
  const a = await access(p);
  const seen = [];
  a.onstatechange = (e) => seen.push([e.port.id, e.port.state, e.port.connection]);
  const input = a.inputs.get('in1');
  input.onmidimessage = () => {};
  seen.length = 0;

  p.event({ event: 'ports', ports: [OUTPUT] });
  assert.deepStrictEqual(seen, [['in1', 'disconnected', 'pending']]);
  assert.strictEqual(a.inputs.size, 1, 'still listed');

  p.event({ event: 'ports', ports: [INPUT, OUTPUT] });
  assert.deepStrictEqual(seen[1], ['in1', 'connected', 'open'], 'picks up where it was');
  assert.strictEqual(p.ops.filter((o) => o.op === 'open').length, 1, 'Kite reconnects it, not the page');

  p.event({ event: 'ports', ports: [INPUT, OUTPUT, { id: 'in2', name: 'New', type: 'input' }] });
  assert.strictEqual(a.inputs.size, 2);
  assert.deepStrictEqual(seen[2], ['in2', 'connected', 'closed']);
});

test('sending to a port that has gone', async () => {
  const p = page();
  const a = await access(p);
  p.event({ event: 'ports', ports: [] });
  assert.throws(() => only(a.outputs).send([0x90, 1, 1]), (e) => e.name === 'InvalidStateError');
});

test('port maps cannot be changed by the page', async () => {
  const a = await access(page());
  assert.strictEqual(a.inputs.set, undefined);
  assert.strictEqual(a.inputs.delete, undefined);
  assert.strictEqual(a.inputs.clear, undefined);
  const seen = [];
  a.inputs.forEach((v, k) => seen.push(k));
  assert.deepStrictEqual(seen, ['in1']);
  assert.strictEqual([...a.inputs][0][0], 'in1');
});

test('permissions.query says what Kite would do', async () => {
  const p = page({ state: 'granted' });
  const q = (d) => p.Permissions.prototype.query.call(p.permissions, d);
  const status = await q({ name: 'midi' });
  assert.strictEqual(status.state, 'granted');
  assert.strictEqual(status.name, 'midi');
  assert.strictEqual((await q({ name: 'midi', sysex: true })).state, 'granted');
  assert.deepStrictEqual(await q({ name: 'camera' }), { native: 'camera' }, 'others are left alone');
});

test('leaving the page stops it, unless it may come back', () => {
  const p = page();
  p.leave(true);
  assert.strictEqual(p.ops.length, 0);
  p.leave(false);
  assert.deepStrictEqual(p.ops, [{ op: 'stop' }]);
});
