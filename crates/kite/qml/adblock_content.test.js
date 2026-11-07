// Run with: node --test crates/kite/qml/adblock_content.test.js
// How procedural filters are read (what they do on a page takes a real one).
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/../src/js/adblock_content.js', 'utf8');

function load(plan = {}, host = 'news.test', made = []) {
  let api = null;
  const sent = [];
  const sandbox = {
    KITE_AB: { host, secret: 'S', plan: { hide: [], procedural: [], generichide: false, exceptions: [], ...plan }, test: (a) => { api = a; } },
    location: { hostname: 'news.test' },
    CSS: { supports: (s) => !/has-text|upward|style\(|:remove|matches-|xpath|bad!/.test(s) },
    crypto: { getRandomValues: (a) => a },
    console: { debug: (m) => sent.push(m) },
    MutationObserver: class { observe() {} disconnect() {} },
    setTimeout: () => 1,
    performance,
    document: {
      documentElement: { appendChild() {} },
      head: null,
      createElement: () => { const e = { textContent: '', isConnected: false }; made.push(e); return e; },
      addEventListener() {},
      querySelectorAll: () => [],
    },
  };
  sandbox.globalThis = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return { api, sent, sandbox };
}
const plain = (x) => JSON.parse(JSON.stringify(x));
const parse = (s) => plain(load().api.parseProcedural(s));

test('only on its own host', () => {
  assert.strictEqual(load({}, 'other.test').api, null);
});

test('an operator after plain CSS', () => {
  assert.deepStrictEqual(parse('div.ad:has-text(Sponsored)'), { base: 'div.ad', tasks: [{ op: 'has-text', arg: 'Sponsored' }], action: null });
});

test('steps in order, with CSS between them', () => {
  assert.deepStrictEqual(parse('a:has-text(x) > span:upward(2)'), {
    base: 'a',
    tasks: [{ op: 'has-text', arg: 'x' }, { op: 'css', arg: ' > span' }, { op: 'upward', arg: '2' }],
    action: null,
  });
});

test('arguments keep their own parentheses and quotes', () => {
  assert.strictEqual(parse('p:has-text(/a(b|c)d/)').tasks[0].arg, '/a(b|c)d/');
  assert.strictEqual(parse('p:has-text("a)b")').tasks[0].arg, '"a)b"');
});

test(':has and :not stay CSS, unless what is inside is not', () => {
  assert.deepStrictEqual(parse('div:has(> a):not(.x)'), { base: 'div:has(> a):not(.x)', tasks: [], action: null });
  assert.deepStrictEqual(parse('div:has(> span:has-text(Sponsored))'), { base: 'div', tasks: [{ op: 'has', arg: '> span:has-text(Sponsored)' }], action: null });
  assert.strictEqual(parse('div:not(:has-text(x))').tasks[0].op, 'not');
});

test('colons elsewhere are not operators', () => {
  assert.strictEqual(parse('a[href^="http:has-text(x)"]').base, 'a[href^="http:has-text(x)"]');
  assert.strictEqual(parse('a[href^="https://x"]:hover').base, 'a[href^="https://x"]:hover');
  assert.strictEqual(parse('.a\\:has-text\\(b').tasks.length, 0);
});

test('actions', () => {
  assert.deepStrictEqual(parse('.x:remove()').action, { type: 'remove', arg: '' });
  assert.deepStrictEqual(parse('.x:style(color: red)').action, { type: 'style', arg: 'color: red' });
  assert.deepStrictEqual(parse('.x:remove-attr(href)').action, { type: 'remove-attr', arg: 'href' });
});

test('what has no end is not understood', () => {
  assert.strictEqual(load().api.parseProcedural('.x:has-text(oops'), null);
});

test('the form the engine gives', () => {
  const { fromEngine } = load().api;
  assert.deepStrictEqual(plain(fromEngine('{"selector":[{"type":"css-selector","arg":".popup"},{"type":"upward","arg":"1"}],"action":{"type":"style","arg":"color: red"}}')), {
    base: '.popup', tasks: [{ op: 'upward', arg: '1' }], action: { type: 'style', arg: 'color: red' },
  });
  assert.strictEqual(fromEngine('nonsense'), null);
  assert.strictEqual(fromEngine('{"selector":1}'), null);
});

test('a selector CSS cannot read does not take the others with it', () => {
  const made = [];
  load({ hide: ['.good', 'bad!', '.also-good', '.x:has-text(y)'] }, 'news.test', made);
  assert.strictEqual(made[0].textContent, '.good,\n.also-good { display: none !important; }\n');
});
