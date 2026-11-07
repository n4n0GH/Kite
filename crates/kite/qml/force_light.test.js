// Run with: node --test crates/kite/qml/force_light.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/../src/js/force_light.js', 'utf8');

const NEVER = '(max-width: -1px)';
const ALWAYS = '(min-width: 0px)';

function media(mediaText) { return { mediaText }; }

// Runs the script against a stand-in for the page, with these style sheets.
function run(styleSheets = [], fetch = async () => ({ ok: false })) {
  const asked = [];
  const page = {
    window: { matchMedia(q) { asked.push(q); return { query: q }; } },
    document: { adoptedStyleSheets: [], styleSheets, addEventListener() {} },
    CSSStyleSheet: class { replaceSync(css) { this.css = css; } },
    MutationObserver: class { observe() {} },
    fetch,
  };
  new Function(...Object.keys(page), src)(...Object.values(page));
  return { ...page, asked };
}

test('scripts asking about the colour scheme are told light', () => {
  const { window, asked } = run();
  window.matchMedia('(prefers-color-scheme: dark)');
  window.matchMedia('(prefers-color-scheme:light)');
  window.matchMedia('screen and (PREFERS-COLOR-SCHEME: DARK)');
  assert.deepStrictEqual(asked, [NEVER, ALWAYS, `screen and ${NEVER}`]);
});

test('other queries are left alone', () => {
  const { window, asked } = run();
  window.matchMedia('(min-width: 600px)');
  assert.deepStrictEqual(asked, ['(min-width: 600px)']);
});

test('the page is given a light colour scheme, keeping its own adopted sheets', () => {
  const { document } = run();
  assert.strictEqual(document.adoptedStyleSheets.length, 1);
  assert.match(document.adoptedStyleSheets[0].css, /color-scheme: light !important/);
});

test('rules are rewritten, also nested and in imported sheets', () => {
  const imported = { media: media('all'), cssRules: [{ media: media('(prefers-color-scheme: light)'), cssRules: [] }] };
  const nested = { media: media('(prefers-color-scheme: dark)'), cssRules: [] };
  const sheet = {
    media: media(''),
    cssRules: [
      { media: media('print and (prefers-color-scheme: dark)'), cssRules: [nested] },
      { media: media('all'), styleSheet: imported },
      { cssRules: [nested] },
      { selectorText: 'body' },
    ],
  };
  run([sheet]);
  assert.strictEqual(sheet.cssRules[0].media.mediaText, `print and ${NEVER}`);
  assert.strictEqual(nested.media.mediaText, NEVER);
  assert.strictEqual(imported.cssRules[0].media.mediaText, ALWAYS);
});

const foreign = (ownerNode) => ({
  media: media('(prefers-color-scheme: dark)'), href: 'https://cdn.example/a.css', ownerNode,
  get cssRules() { throw new Error('SecurityError'); },
});
const settled = () => new Promise((r) => setImmediate(r));

test('a sheet from another origin still has its media fixed, without an error', () => {
  const sheet = foreign(null);
  assert.doesNotThrow(() => run([sheet], async () => ({ ok: false })));
  assert.strictEqual(sheet.media.mediaText, NEVER);
});

// A stand-in <link>, recording what the script does with it.
function linkTag(crossOrigin = null) {
  const link = {
    nodeName: 'LINK', crossOrigin, removed: false, added: null,
    cloneNode() {
      const copy = { crossOrigin: this.crossOrigin, addEventListener(type, fn) { this[type] = fn; } };
      return copy;
    },
    after(copy) { this.added = copy; },
    remove() { this.removed = true; },
  };
  return link;
}

test('another origin\'s sheet loads readably if its server allows that', async () => {
  const link = linkTag();
  run([foreign(link)], async (url, opts) => {
    assert.strictEqual(url, 'https://cdn.example/a.css');
    assert.strictEqual(opts.mode, 'cors');
    return { ok: true };
  });
  await settled();
  assert.strictEqual(link.added.crossOrigin, 'anonymous');
  assert.strictEqual(link.removed, false, 'the original stays until the copy has loaded');
  link.added.load();
  assert.strictEqual(link.removed, true);
});

test('another origin\'s sheet is left alone if its server doesn\'t allow that', async () => {
  const link = linkTag();
  run([foreign(link)], () => Promise.reject(new TypeError('Failed to fetch')));
  await settled();
  assert.strictEqual(link.added, null);
});

test('a sheet already loaded for reading isn\'t asked for again', () => {
  let asked = 0;
  run([foreign(linkTag('anonymous'))], async () => { asked++; return { ok: true }; });
  assert.strictEqual(asked, 0);
});
