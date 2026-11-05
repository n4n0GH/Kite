// Run with: node --test crates/kite/qml/palette.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/palette.js', 'utf8').replace('.pragma library', '');
const P = new Function(src + '\nreturn {TOGGLES, CHOICES, PAGE_TITLES, ACTION_ICONS};')();

// The plain checkboxes of the Settings window.
const prefs = fs.readFileSync(__dirname + '/PreferencesWindow.qml', 'utf8');
const inSettings = [];
for (const line of prefs.split('\n')) {
  const m = /Check \{.*?text: "([^"]+)"; key: "([\w.]+)"/.exec(line) || /QQC2\.CheckBox \{.*?text: "([^"]+)"; checked: w\.get\("([\w.]+)"\)/.exec(line);
  if (m) inSettings.push({ text: m[1], key: m[2] });
}

test('every checkbox in Settings can be switched from the palette', () => {
  assert.ok(inSettings.length > 40, 'found the checkboxes: ' + inSettings.length);
  const known = new Set(P.TOGGLES.map((t) => t.key));
  assert.deepStrictEqual(inSettings.filter((c) => !known.has(c.key)), []);
});

test('the palette only names what Settings has, in its words', () => {
  const there = new Map(inSettings.map((c) => [c.key, c.text]));
  for (const t of P.TOGGLES) assert.strictEqual(there.get(t.key), t.text, t.key);
});

test('settings exist, and are switches', () => {
  const defaults = fs.readFileSync(__dirname + '/../../kite-core/src/settings.rs', 'utf8');
  for (const t of P.TOGGLES) {
    const m = new RegExp('"' + t.key.replace('.', '\\.') + '":\\s*(\\w+)').exec(defaults);
    assert.ok(m, t.key + ' has a default');
    assert.ok(m[1] === 'true' || m[1] === 'false', t.key + ' is on or off');
  }
});

test('pages exist in the Settings window', () => {
  for (const item of [...P.TOGGLES, ...P.CHOICES]) {
    assert.ok(P.PAGE_TITLES[item.page], item.page);
    assert.ok(prefs.includes('key: "' + item.page + '", title: "' + P.PAGE_TITLES[item.page] + '"'), item.page);
  }
});

test('nothing is listed twice', () => {
  const names = [...P.TOGGLES, ...P.CHOICES].map((i) => i.page + i.text);
  assert.strictEqual(new Set(names).size, names.length);
});

test('shortcut actions have icons', () => {
  const main = fs.readFileSync(__dirname + '/Main.qml', 'utf8');
  const ids = [...main.matchAll(/\{id: "(\w+)", group: "/g)].map((m) => m[1]).filter((id) => !/^tab(\d|Last)/.test(id));
  assert.ok(ids.length > 30);
  assert.deepStrictEqual(ids.filter((id) => !P.ACTION_ICONS[id]), []);
});
