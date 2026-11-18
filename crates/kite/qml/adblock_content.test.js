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

// An element with just enough of a style declaration: `display` and its priority.
function element(selectorMatched, display = '', priority = '') {
  const decl = { display, priority };
  return {
    removed: false,
    matches: () => selectorMatched,
    hasAttribute: (name) => name === 'style' && decl.display !== '',
    remove() { this.removed = true; },
    style: {
      getPropertyValue: (p) => (p === 'display' ? decl.display : ''),
      getPropertyPriority: (p) => (p === 'display' ? decl.priority : ''),
      setProperty(p, v, prio = '') { if (p === 'display') { decl.display = v; decl.priority = prio; } },
      removeProperty(p) { if (p === 'display') { decl.display = ''; decl.priority = ''; } },
    },
    decl,
  };
}

test('an element insisting on being shown is held hidden, and again when it is taken back', () => {
  const { recheck } = load({ hide: ['#fullscreen-ad-trap'] }).api;
  // kaa.lt: `display: block !important` on the element itself, which the sheet can't overrule.
  const trap = element(true, 'block', 'important');
  recheck(trap);
  assert.deepStrictEqual(trap.decl, { display: 'none', priority: 'important' });
  trap.style.setProperty('display', 'block', 'important');
  recheck(trap);
  assert.deepStrictEqual(trap.decl, { display: 'none', priority: 'important' });
});

test('only what a rule names, and only while it names it', () => {
  const { recheck } = load({ hide: ['.ad'] }).api;
  const other = element(false, 'block', 'important');
  recheck(other);
  assert.strictEqual(other.decl.display, 'block');
  // (No reason to touch an element the sheet already hides.)
  const plainAd = element(true);
  recheck(plainAd);
  assert.strictEqual(plainAd.decl.display, '');
  // Held, then its class changed: it's let go.
  const was = element(true, 'flex', 'important');
  recheck(was);
  was.matches = () => false;
  recheck(was);
  assert.deepStrictEqual(was.decl, { display: '', priority: '' });
});

test('an element taken back over and over goes', () => {
  const { recheck } = load({ hide: ['.fight'] }).api;
  const el = element(true, 'block', 'important');
  for (let i = 0; i < 25 && !el.removed; i++) {
    recheck(el);
    el.style.setProperty('display', 'block', 'important');
  }
  assert.ok(el.removed);
});

// ---- watching the page ----
// The script with a page's changes to be told of (`changed`), its timers to let run (`look`)
// and the idle moment it waits for (`idle`). `page.query`: what the document finds for a selector.
function watching(plan = {}) {
  let api = null;
  const sent = [], timers = [], idles = [], observers = [], queries = [];
  const page = { query: () => [] };
  const sandbox = {
    KITE_AB: { host: 'news.test', secret: 'S', plan: { hide: [], procedural: [], generichide: false, exceptions: [], ...plan }, test: (a) => { api = a; } },
    location: { hostname: 'news.test' },
    CSS: { supports: () => true },
    crypto: { getRandomValues: (a) => a },
    console: { debug: (m) => sent.push(JSON.parse(m.slice('Sab:'.length))) },
    MutationObserver: class { constructor(cb) { this.cb = cb; observers.push(this); } observe() {} disconnect() {} },
    setTimeout: (f) => timers.push(f),
    clearTimeout() {},
    requestIdleCallback: (f) => idles.push(f),
    getComputedStyle: (el) => ({ display: el.decl.display || (el.sheetHides ? 'none' : 'block') }),
    performance,
    document: {
      documentElement: { appendChild() {} },
      head: null,
      createElement: () => ({ textContent: '', isConnected: false }),
      addEventListener() {},
      querySelectorAll: (selector) => { queries.push(selector); return page.query(selector); },
    },
  };
  sandbox.globalThis = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  const run = (list) => { for (const f of list.splice(0)) f(); };
  const w = {
    api, sent, queries, page, sandbox,
    changed: (records) => observers[0].cb(records),
    look: () => run(timers),
    idle: () => run(idles),
  };
  // (The first look is at the document as it was found.)
  w.look();
  queries.length = 0;
  return w;
}
// An element of such a page. `named`: a hiding rule names it; `sheetHides`: and the sheet gets its way.
function node({ id = null, classes = null, inside = [], connected = true, named = false, sheetHides = true } = {}) {
  const el = element(named);
  return Object.assign(el, {
    nodeType: 1,
    isConnected: connected,
    sheetHides: named && sheetHides,
    firstElementChild: inside[0] || null,
    getAttribute: (name) => (name === 'id' ? id : name === 'class' ? classes : null),
    querySelectorAll: (selector) => (selector === '[style]' ? [] : selector === '[id],[class]' ? inside : inside.filter((e) => e.matches())),
  });
}
const adding = (...nodes) => ({ type: 'childList', addedNodes: nodes });
const renaming = (target) => ({ type: 'attributes', attributeName: 'class', target });

test('classes and ids are asked about at the next look, each once', () => {
  const w = watching();
  w.changed([adding(node({ id: 'top', classes: 'box  wide', inside: [node({ classes: 'ad-slot' })] }))]);
  assert.strictEqual(w.sent.length, 0, 'not as it happens');
  w.look();
  assert.deepStrictEqual(plain(w.sent), [{ f: w.sent[0].f, c: ['box', 'wide', 'ad-slot'], i: ['top'], x: [] }]);
  w.changed([adding(node({ id: 'top', classes: 'wide box' }), node({ classes: 'ad-slot' }))]);
  w.look();
  assert.strictEqual(w.sent.length, 1);
  w.changed([renaming(node({ classes: 'box open' }))]);
  w.look();
  assert.deepStrictEqual(plain(w.sent[1].c), ['open']);
});

test('what the page took away again before the look is not asked about', () => {
  const w = watching();
  w.changed([adding(node({ classes: 'gone', connected: false }), node({ classes: 'here' }))]);
  w.look();
  assert.deepStrictEqual(plain(w.sent[0].c), ['here']);
});

test('after very many changes the whole document is read once instead', () => {
  const w = watching();
  const many = [];
  for (let i = 0; i < 2500; i++) many.push(node({ classes: 'row' }));
  w.page.query = (selector) => (selector === '[id],[class]' ? [node({ classes: 'row' }), node({ id: 'list' })] : []);
  w.changed([adding(...many)]);
  w.look();
  assert.deepStrictEqual(w.queries, ['[id],[class]']);
  assert.deepStrictEqual(plain([w.sent[0].c, w.sent[0].i]), [['row'], ['list']]);
});

test('nothing is asked where generic rules are off', () => {
  const w = watching({ generichide: true, hide: ['.ad'] });
  w.changed([adding(node({ classes: 'box' }))]);
  w.look();
  assert.strictEqual(w.sent.length, 0);
});

test('a new element a rule names that the page keeps shown is held at the next look', () => {
  const w = watching({ hide: ['.ad'] });
  const forced = node({ classes: 'ad d-block', named: true, sheetHides: false });
  const hidden = node({ classes: 'ad', named: true });
  const within = node({ classes: 'ad d-flex', named: true, sheetHides: false });
  w.changed([adding(forced, hidden, node({ inside: [within] }))]);
  w.look();
  assert.deepStrictEqual(forced.decl, { display: 'none', priority: 'important' });
  assert.deepStrictEqual(hidden.decl, { display: '', priority: '' });
  assert.deepStrictEqual(within.decl, { display: 'none', priority: 'important' });
  assert.ok(!w.queries.includes('.ad'), 'without going through the document');
});

test('the whole document is gone through when the page has nothing to do', () => {
  const w = watching({ hide: ['.ad'] });
  const old = node({ classes: 'ad', named: true, sheetHides: false });
  w.page.query = (selector) => (selector === '.ad' ? [old] : []);
  w.changed([renaming(node({ classes: 'theme-b' }))]);
  w.look();
  w.look();
  assert.strictEqual(old.decl.display, '', 'not yet');
  w.idle();
  assert.deepStrictEqual(old.decl, { display: 'none', priority: 'important' });
});

test('selectors that come later hold what they name', () => {
  const w = watching();
  const old = node({ classes: 'sponsor', named: true, sheetHides: false });
  w.changed([adding(old)]);
  w.look();
  w.page.query = (selector) => (selector === '.sponsor' ? [old] : []);
  w.sandbox.__kiteAbFill(w.sent[0].f, ['.sponsor']);
  assert.deepStrictEqual(old.decl, { display: 'none', priority: 'important' });
  assert.deepStrictEqual(w.queries, ['.sponsor']);
});
