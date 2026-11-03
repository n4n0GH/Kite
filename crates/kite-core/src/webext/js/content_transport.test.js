// Run with: node --test crates/kite-core/src/webext/js/content_transport.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const core = fs.readFileSync(__dirname + '/api_core.js', 'utf8');
const transport = fs.readFileSync(__dirname + '/content_transport.js', 'utf8');

test('content scripts in sub-frames call the runtime over the extension origin', async () => {
  const requests = [];
  let deliver = null; // resolves the held poll
  const g = {
    location: { href: 'https://ads.example/frame.html', ancestorOrigins: ['https://ads.example', 'https://www.youtube.com'] },
    document: { dispatchEvent() { throw new Error('no bridge in sub-frames'); }, addEventListener() {} },
    addEventListener() {},
    CustomEvent: class {},
    setTimeout,
    fetch: (url, init) => {
      const body = JSON.parse(init.body);
      requests.push({ url, body });
      if (url.endsWith('/poll')) return new Promise((res) => { deliver = (msgs) => res({ status: 200, json: async () => msgs }); });
      return Promise.resolve({ status: 200, json: async () => ({}) });
    },
  };
  g.window = g; g.top = {}; g.globalThis = g;
  const cfg = { id: 'v@x', uuid: 'u1', token: 'secret', kind: 'content', locale: 'en', manifest: {}, messages: {} };
  new Function('globalThis', 'window', 'top', 'document', 'location', 'fetch', 'addEventListener', 'CustomEvent',
    core + '\n;' + transport.replace('__KITE_CFG__', JSON.stringify(cfg)))(g, g, g.top, g.document, g.location, g.fetch, g.addEventListener, g.CustomEvent);

  // Polls first, then registers with its own frame id; nothing secret in URLs.
  assert.strictEqual(requests[0].url, 'moz-extension://u1/__kite__/poll');
  assert.strictEqual(requests[1].url, 'moz-extension://u1/__kite__/rpc');
  assert.strictEqual(requests[1].body.method, '_.register');
  assert.ok(requests[1].body.args[0].frameId > 0);
  assert.strictEqual(requests[1].body.token, 'secret');
  assert.strictEqual(requests[1].body.top, 'https://www.youtube.com');
  assert.ok(requests.every((r) => !r.url.includes('secret') && !r.url.includes('?')));

  const got = g.chrome.storage.session.get('k');
  const call = requests[2];
  assert.strictEqual(call.body.method, 'storage.session.get');
  deliver([{ kind: 'reply', ctx: call.body.ctx, callId: call.body.callId, ok: true, value: { k: 1 } }]);
  assert.deepStrictEqual(await got, { k: 1 });
});
