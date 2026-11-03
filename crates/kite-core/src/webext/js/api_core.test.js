// Run with: node --test crates/kite-core/src/webext/js/
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/api_core.js', 'utf8');
const __kiteMakeAPI = new Function(src + '\nreturn __kiteMakeAPI;')();

function make(kind = 'background') {
  const calls = [];
  const replies = {};
  const transport = { call: (m, a) => { calls.push([m, a]); return Promise.resolve(m in replies ? replies[m] : null); } };
  const cfg = { id: 'x@y', uuid: 'abcd-1234', token: 't', kind, locale: 'en_US', manifest: { name: 'X', version: '1' },
    messages: { hello: { message: 'Hi $who$ ($1)', placeholders: { who: { content: '$2' } } } } };
  return { api: __kiteMakeAPI(cfg, transport), calls, replies };
}

test('sync APIs', () => {
  const { api } = make();
  assert.strictEqual(api.browser.runtime.getURL('/a/b.html'), 'moz-extension://abcd-1234/a/b.html');
  assert.strictEqual(api.browser.i18n.getMessage('HELLO', ['3', 'Bob']), 'Hi Bob (3)');
  assert.strictEqual(api.browser.i18n.getMessage('@@extension_id'), 'x@y');
  assert.strictEqual(api.browser.i18n.getUILanguage(), 'en-US');
  assert.deepStrictEqual(api.browser.runtime.getManifest(), { name: 'X', version: '1' });
});

test('content scripts get a restricted API', () => {
  const { api } = make('content');
  assert.ok(api.browser.runtime.sendMessage);
  assert.strictEqual(api.browser.tabs, undefined);
});

test('calls, errors and chrome callbacks with lastError', async () => {
  const { api, calls, replies } = make();
  replies['tabs.query'] = [{ id: 1 }];
  assert.deepStrictEqual(await api.browser.tabs.query({ active: true }), [{ id: 1 }]);
  assert.deepStrictEqual(calls.at(-1), ['tabs.query', [{ active: true }]]);
  const res = await new Promise((r) => api.chrome.tabs.query({}, r));
  assert.deepStrictEqual(res, [{ id: 1 }]);
  replies['storage.local.get'] = { __kiteError: 'boom' };
  await assert.rejects(api.browser.storage.local.get('k'), /boom/);
  const err = await new Promise((r) => api.chrome.storage.local.get('k', () => r(api.chrome.runtime.lastError)));
  assert.strictEqual(err.message, 'boom');
  assert.strictEqual(api.chrome.runtime.lastError, null);
});

test('runtime.sendMessage argument normalisation', async () => {
  const { api, calls } = make();
  await api.browser.runtime.sendMessage({ a: 1 });
  assert.deepStrictEqual(calls.at(-1), ['runtime.sendMessage', ['', { a: 1 }]]);
  await api.browser.runtime.sendMessage('other@ext', 'hi');
  assert.deepStrictEqual(calls.at(-1), ['runtime.sendMessage', ['other@ext', 'hi']]);
  await api.browser.runtime.sendMessage('just a string');
  assert.deepStrictEqual(calls.at(-1), ['runtime.sendMessage', ['', 'just a string']]);
});

test('onMessage: sendResponse, promises and no handler', async () => {
  const { api, calls } = make();
  api.browser.runtime.onMessage.addListener((m, s, sendResponse) => { if (m === 'cb') { setTimeout(() => sendResponse('pong'), 1); return true; } });
  api.browser.runtime.onMessage.addListener((m) => (m === 'p' ? Promise.resolve(42) : undefined));
  api.dispatch({ type: 'message', msgId: 1, message: 'cb', sender: {} });
  api.dispatch({ type: 'message', msgId: 2, message: 'p', sender: {} });
  api.dispatch({ type: 'message', msgId: 3, message: 'nobody', sender: {} });
  await new Promise((r) => setTimeout(r, 10));
  const responds = calls.filter((c) => c[0] === '_.respond').map((c) => c[1]);
  assert.deepStrictEqual(responds.find((r) => r[0] === 1), [1, 'pong', true]);
  assert.deepStrictEqual(responds.find((r) => r[0] === 2), [2, 42, true]);
  assert.deepStrictEqual(responds.find((r) => r[0] === 3), [3, null, false]);
});

test('ports', async () => {
  const { api, calls } = make();
  let got = null, port = null;
  api.browser.runtime.onConnect.addListener((p) => { port = p; p.onMessage.addListener((m) => (got = m)); });
  api.dispatch({ type: 'connect', portId: 'P', name: 'n', sender: { id: 'x@y' } });
  assert.strictEqual(port.name, 'n');
  api.dispatch({ type: 'portMessage', portId: 'P', message: { v: 1 } });
  assert.deepStrictEqual(got, { v: 1 });
  port.postMessage('back');
  assert.deepStrictEqual(calls.at(-1), ['_.portPost', ['P', 'back']]);
  let disc = false;
  const p2 = api.browser.runtime.connect({ name: 'x' });
  p2.onDisconnect.addListener(() => (disc = true));
  api.dispatch({ type: 'portDisconnect', portId: p2._id, error: 'gone' });
  assert.ok(disc);
  assert.throws(() => p2.postMessage(1));
});

test('event filters', () => {
  const { api } = make();
  const seen = [];
  api.browser.tabs.onUpdated.addListener((id) => seen.push(id), { properties: ['status'], urls: ['*://*.kde.org/*'] });
  api.dispatch({ type: 'event', name: 'tabs.onUpdated', args: [1, { status: 'complete' }, { id: 1, url: 'https://kde.org/' }] });
  api.dispatch({ type: 'event', name: 'tabs.onUpdated', args: [2, { title: 'x' }, { id: 2, url: 'https://kde.org/' }] });
  api.dispatch({ type: 'event', name: 'tabs.onUpdated', args: [3, { status: 'complete' }, { id: 3, url: 'https://gnome.org/' }] });
  assert.deepStrictEqual(seen, [1]);
  const nav = [];
  api.browser.webNavigation.onCompleted.addListener((d) => nav.push(d.url), { url: [{ hostSuffix: 'kde.org' }] });
  api.dispatch({ type: 'event', name: 'webNavigation.onCompleted', args: [{ url: 'https://apps.kde.org/' }] });
  api.dispatch({ type: 'event', name: 'webNavigation.onCompleted', args: [{ url: 'https://example.com/' }] });
  assert.deepStrictEqual(nav, ['https://apps.kde.org/']);
});

test('blocking webRequest listeners answer over the hub socket', async () => {
  const sockets = [];
  globalThis.WebSocket = class { constructor(url) { this.url = url; this.sent = []; sockets.push(this); } send(m) { this.sent.push(JSON.parse(m)); } close() {} };
  const calls = [];
  const transport = { call: (m, a) => { calls.push([m, a]); return Promise.resolve(null); } };
  const cfg = { id: 'x@y', uuid: 'u', token: 'tok', kind: 'page', blockingPort: 4321, locale: 'en', manifest: {}, messages: {} };
  const { browser, dispatch } = __kiteMakeAPI(cfg, transport);
  const seen = [];
  browser.webRequest.onBeforeRequest.addListener((d) => (d.url.includes('ads') ? { cancel: true } : undefined), { urls: ['<all_urls>'], types: ['script'] }, ['blocking']);
  browser.webRequest.onBeforeRequest.addListener(async () => ({ redirectUrl: 'moz-extension://u/noop.js' }), { urls: ['*://*.tracker.test/*'] }, ['blocking']);
  browser.webRequest.onBeforeRequest.addListener((d) => seen.push(d.url), { urls: ['<all_urls>'] });
  assert.strictEqual(sockets.length, 1);
  assert.strictEqual(sockets[0].url, 'ws://127.0.0.1:4321/tok');
  assert.deepStrictEqual(calls[0][1][2], ['blocking']);
  assert.deepStrictEqual(calls[2][1][2], []);

  const ws = sockets[0];
  ws.onmessage({ data: JSON.stringify({ id: 1, event: 'onBeforeRequest', details: { url: 'https://ads.example/a.js', type: 'script' } }) });
  assert.deepStrictEqual(ws.sent.pop(), { id: 1, result: { cancel: true } });
  ws.onmessage({ data: JSON.stringify({ id: 2, event: 'onBeforeRequest', details: { url: 'https://cdn.tracker.test/t.js', type: 'image' } }) });
  await new Promise((r) => setTimeout(r, 0));
  assert.deepStrictEqual(ws.sent.pop(), { id: 2, result: { redirectUrl: 'moz-extension://u/noop.js' } });
  ws.onmessage({ data: JSON.stringify({ id: 3, event: 'onBeforeRequest', details: { url: 'https://ok.test/', type: 'image' } }) });
  assert.deepStrictEqual(ws.sent.pop(), { id: 3, result: null });
  // Only the non-blocking listener sees the event.
  dispatch({ type: 'event', name: 'webRequest.onBeforeRequest', args: [{ url: 'https://ok.test/', type: 'image' }] });
  assert.deepStrictEqual(seen, ['https://ok.test/']);
  delete globalThis.WebSocket;
});
