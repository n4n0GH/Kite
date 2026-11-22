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

test('browser.* takes callbacks too, as Firefox does', async () => {
  const { api, calls, replies } = make('content');
  // (As the Unhook add-on reads its settings: nothing came of it without the callback.)
  replies['storage.sync.get'] = { hide_feed: true };
  const got = await new Promise((r) => { assert.strictEqual(api.browser.storage.sync.get(null, r), undefined); });
  assert.deepStrictEqual(got, { hide_feed: true });
  assert.deepStrictEqual(calls.at(-1), ['storage.sync.get', [null]], 'the callback is not sent along as an argument');
  await new Promise((r) => api.browser.storage.sync.set({ hide_feed: false }, r));
  assert.deepStrictEqual(calls.at(-1), ['storage.sync.set', [{ hide_feed: false }]]);
  // Without one, a promise as before.
  assert.deepStrictEqual(await api.browser.storage.sync.get(null), { hide_feed: true });
  // A call that fails: lastError while the callback runs, on either name.
  replies['storage.sync.get'] = { __kiteError: 'boom' };
  const err = await new Promise((r) => api.browser.storage.sync.get('k', (v) => r([v, api.browser.runtime.lastError, api.chrome.runtime.lastError])));
  assert.strictEqual(err[0], undefined);
  assert.strictEqual(err[1].message, 'boom');
  assert.strictEqual(err[2].message, 'boom');
  assert.strictEqual(api.browser.runtime.lastError, null);
  // A reply to a message, the same way.
  replies['runtime.sendMessage'] = 'pong';
  assert.strictEqual(await new Promise((r) => api.browser.runtime.sendMessage({ ping: 1 }, r)), 'pong');
  assert.deepStrictEqual(calls.at(-1), ['runtime.sendMessage', ['', { ping: 1 }]]);
  // The two names are objects of their own with the same events.
  assert.notStrictEqual(api.browser, api.chrome);
  assert.strictEqual(api.browser.storage.onChanged, api.chrome.storage.onChanged);
  assert.strictEqual(api.browser.runtime.onMessage, api.chrome.runtime.onMessage);
});

test('menus.create answers with the id and calls back, in both styles', async () => {
  const { api, calls, replies } = make();
  // (As Bitwarden does: the next entry is only made once this one has called back.)
  let id;
  await new Promise((r) => { id = api.chrome.contextMenus.create({ id: 'root', title: 'Root', contexts: ['all'] }, r); });
  assert.strictEqual(id, 'root');
  assert.deepStrictEqual(calls.at(-1), ['menus.create', [{ id: 'root', title: 'Root', contexts: ['all'] }]]);
  await new Promise((r) => api.browser.menus.create({ id: 'child', parentId: 'root', title: 'Child' }, r));
  assert.strictEqual(api.chrome.contextMenus.create({ id: 'quiet', title: 'No callback' }), 'quiet');
  replies['menus.create'] = { __kiteError: 'ID already exists: root' };
  const err = await new Promise((r) => api.chrome.contextMenus.create({ id: 'root', title: 'Again' }, () => r(api.chrome.runtime.lastError)));
  assert.strictEqual(err.message, 'ID already exists: root');
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

test('a connection from another extension is not one of its own', () => {
  const { api, calls } = make();
  const own = [], external = [];
  api.browser.runtime.onConnect.addListener((p) => own.push(p.name));
  api.dispatch({ type: 'connectExternal', portId: 'E1', name: 'theirs', sender: { id: 'other@x' } });
  // Nobody listens for those: the port is closed, and the extension's own listener heard nothing.
  assert.deepStrictEqual(own, []);
  assert.deepStrictEqual(calls.at(-1)[0], '_.portDisconnect');
  api.browser.runtime.onConnectExternal.addListener((p) => external.push(p.name + ' from ' + p.sender.id));
  api.dispatch({ type: 'connectExternal', portId: 'E2', name: 'theirs', sender: { id: 'other@x' } });
  api.dispatch({ type: 'connect', portId: 'O1', name: 'mine', sender: { id: 'x@y' } });
  assert.deepStrictEqual(external, ['theirs from other@x']);
  assert.deepStrictEqual(own, ['mine']);
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
  // The token is the first message, not part of the address (which other
  // extensions' webRequest listeners would be shown).
  assert.strictEqual(sockets[0].url, 'ws://127.0.0.1:4321/');
  sockets[0].onopen();
  assert.deepStrictEqual(sockets[0].sent.pop(), { token: 'tok' });
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

test('native messaging: a port to the host, and one-off messages', async () => {
  const { api, calls } = make();
  const port = api.browser.runtime.connectNative('org.host');
  assert.strictEqual(port.name, 'org.host');
  assert.strictEqual(calls[0][0], 'runtime.connectNative');
  assert.strictEqual(calls[0][1][1], 'org.host');
  const got = [];
  port.onMessage.addListener((m) => got.push(m));
  api.dispatch({ type: 'portMessage', portId: port._id, message: { a: 1 } });
  assert.deepStrictEqual(got, [{ a: 1 }]);
  port.postMessage({ b: 2 });
  assert.deepStrictEqual(calls[1], ['_.portPost', [port._id, { b: 2 }]]);
  await api.browser.runtime.sendNativeMessage('org.host', { c: 3 });
  assert.deepStrictEqual(calls[2], ['runtime.sendNativeMessage', ['org.host', { c: 3 }]]);
  // chrome.*: a port too, not a promise.
  const chromePort = api.chrome.runtime.connectNative('org.host');
  assert.ok(chromePort.postMessage);
});

test('native messaging: a refused host disconnects the port with the reason', async () => {
  const calls = [];
  const transport = { call: (m) => { calls.push(m); return m === 'runtime.connectNative' ? Promise.reject(new Error('no such host')) : Promise.resolve(null); }, listen() {} };
  const api = __kiteMakeAPI({ id: 'x@y', uuid: 'abcd-1234', token: 't', kind: 'background', locale: 'en_US', manifest: {}, messages: {} }, transport);
  const port = api.browser.runtime.connectNative('nope');
  const seen = await new Promise((res) => port.onDisconnect.addListener((p) => res([p.error, api.browser.runtime.lastError])));
  assert.deepStrictEqual(seen, [{ message: 'no such host' }, { message: 'no such host' }]);
  assert.throws(() => port.postMessage(1));
});
