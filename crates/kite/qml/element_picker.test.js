// Run with: node --test crates/kite/qml/element_picker.test.js
// What a picked element gets called (the rest takes a real page).
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/../src/js/element_picker.js', 'utf8');

function load() {
  let api = null;
  const noop = () => {};
  const element = () => ({ style: {}, attachShadow: () => ({ append: noop, querySelectorAll: () => [] }), append: noop, remove: noop });
  const sandbox = {
    document: { createElement: element, documentElement: element(), querySelectorAll: () => [] },
    addEventListener: noop, removeEventListener: noop, requestAnimationFrame: noop, cancelAnimationFrame: noop,
    CSS: { escape: (s) => s.replace(/[^\w-]/g, '\\$&') },
    __kitePickerTest: (a) => { api = a; },
    location: { hostname: 'news.test' },
  };
  sandbox.globalThis = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return api;
}
function el(tag, attrs = {}, parent = null, previous = null) {
  return {
    localName: tag,
    parentElement: parent,
    previousElementSibling: previous,
    getAttribute: (n) => (n in attrs ? attrs[n] : null),
  };
}
const { selectorOf, stableClasses, cosmeticPart } = load();

test('an id, if it looks like it lasts', () => {
  assert.strictEqual(selectorOf(el('div', { id: 'sidebar-ad' })), '#sidebar-ad');
  assert.strictEqual(selectorOf(el('div', { id: 'ad-8273645', class: 'banner' })), 'div.banner', 'a generated id');
  assert.strictEqual(selectorOf(el('div', { id: 'deadbeefcafe', class: 'banner' })), 'div.banner');
});

test('classes, without the ones that come and go', () => {
  assert.strictEqual(selectorOf(el('section', { class: 'promo wide' })), 'section.promo.wide');
  assert.deepStrictEqual([...stableClasses(el('div', { class: 'ad hover is-open x12345 active' }))], ['ad']);
});

test('what an image or frame loads, without its query', () => {
  assert.strictEqual(selectorOf(el('img', { src: 'https://ads.example/a.png?x=1' })), 'img[src^="https://ads.example/a.png"]');
  assert.strictEqual(selectorOf(el('iframe', { src: 'https://ads.example/frame#top' })), 'iframe[src^="https://ads.example/frame"]');
  assert.strictEqual(selectorOf(el('img', { src: 'data:image/png;base64,xx', alt: 'Ad' })), 'img[alt="Ad"]', 'a data address says nothing');
});

test('an attribute, then its place under its parent', () => {
  assert.strictEqual(selectorOf(el('div', { 'aria-label': 'Sponsored "now"' })), 'div[aria-label="Sponsored \\"now\\""]');
  const parent = el('ul', { class: 'list' });
  const first = el('li', {}, parent);
  const second = el('li', {}, parent, first);
  assert.strictEqual(selectorOf(second), 'ul.list > li:nth-of-type(2)');
});

test('the part of a rule that can be tried here', () => {
  assert.strictEqual(cosmeticPart('news.test##.ad'), '.ad');
  assert.strictEqual(cosmeticPart('##.ad > div'), '.ad > div');
  assert.strictEqual(cosmeticPart('||ads.example^'), null);
  assert.strictEqual(cosmeticPart('news.test##+js(set, a, 1)'), null);
  assert.strictEqual(cosmeticPart('news.test##^script'), null);
});
