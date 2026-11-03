// Run with: node --test crates/kite-core/src/webext/js/qwebchannel_patch.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const patch = fs.readFileSync(__dirname + '/qwebchannel_patch.js', 'utf8');

// Minimal stand-in for Qt's client: numbers requests from 0, init is id 0.
function StockQWebChannel(transport, initCallback) {
  const ch = this;
  ch.execCallbacks = {}; ch.execId = 0; ch.objects = {}; ch.log = [];
  ch.exec = (data, cb) => { data.id = ch.execId++; ch.execCallbacks[data.id] = cb; transport.sent.push(data); };
  ch.handleResponse = (m) => { ch.execCallbacks[m.id](m.data); delete ch.execCallbacks[m.id]; };
  ch.handleSignal = (m) => ch.log.push('signal ' + m.object + '::' + m.signal);
  ch.handlePropertyUpdate = (m) => ch.log.push('update');
  ch.exec({ type: 'init' }, (data) => { ch.objects = data; if (initCallback) initCallback(ch); });
}
const load = () => new Function('QWebChannel', patch + '\nreturn QWebChannel;')(StockQWebChannel);

test('ignores messages meant for the previous document', () => {
  const QWebChannel = load();
  const transport = { sent: [] };
  const ch = new QWebChannel(transport, null);
  // Before init: old signals/updates and replies to unknown ids are dropped.
  ch.handleSignal({ object: 'kite', signal: 8 });
  ch.handlePropertyUpdate({ data: [] });
  ch.handleResponse({ id: 3, data: 'old reply' });
  assert.deepStrictEqual(ch.log, []);
  ch.handleResponse({ id: 0, data: { kite: {} } });
  assert.ok(ch.objects.kite);
  // Requests are numbered from a random offset, so an old document's reply
  // (small ids) can't reach a pending call of this one.
  let got = null;
  ch.exec({ type: 'invoke' }, (d) => { got = d; });
  const id = transport.sent[1].id;
  assert.ok(id > 1);
  ch.handleResponse({ id: 1, data: 'old reply' });
  assert.strictEqual(got, null);
  ch.handleResponse({ id, data: 'mine' });
  assert.strictEqual(got, 'mine');
  ch.handleSignal({ object: 'kite', signal: 8 });
  assert.deepStrictEqual(ch.log, ['signal kite::8']);
});
