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

// ---- what comes first when searching, with the real names ----
const fuzzy = new Function(fs.readFileSync(__dirname + '/fuzzy.js', 'utf8').replace('.pragma library', '') + '\nreturn {match};')();
const WEIGHT = new Function(src + '\nreturn SETTING_WEIGHT;')();
const main = fs.readFileSync(__dirname + '/Main.qml', 'utf8');
const actions = [...main.matchAll(/\{id: "\w+", group: "([^"]+)", name: "([^"]+)"/g)].map((m) => ({ label: m[1] + ': ' + m[2], weight: 1 }));
const settings = P.TOGGLES.concat(P.CHOICES).map((t) => ({ label: 'Settings: ' + P.PAGE_TITLES[t.page] + (t.prefix ? ', ' + t.prefix : '') + ': ' + t.text, weight: WEIGHT }));
// As the command bar does it.
function search(query) {
  const out = [];
  for (const r of actions.concat(settings)) { const m = fuzzy.match(query, r.label); if (m) out.push({ label: r.label, score: m.score * r.weight, setting: r.weight !== 1 }); }
  out.sort((a, b) => b.score - a.score);
  const best = out.reduce((top, r) => Math.max(top, r.score), 0);
  return out.filter((r) => r.score >= best / 3);
}

test('actions come before settings matching as well', () => {
  assert.ok(actions.length > 40, 'found the actions: ' + actions.length);
  const found = search('priva');
  assert.match(found[0].label, /private/i);
  assert.strictEqual(found[0].setting, false);
  const firstSetting = found.findIndex((r) => r.setting);
  assert.ok(found.slice(0, firstSetting < 0 ? undefined : firstSetting).every((r) => /private/i.test(r.label)), JSON.stringify(found.slice(0, 5)));
  assert.ok(found.some((r) => r.setting), 'settings stay in the list');
});

test('a setting is still found first when only its words match', () => {
  for (const [query, expected] of [['https', /HTTPS-only/], ['javascript', /Enable JavaScript/], ['dark mode', /dark mode/i], ['referrer', /referrer/], ['spell', /spelling/i], ['fingerprint', /fingerprint/]]) {
    assert.match(search(query)[0].label, expected, query + ': ' + JSON.stringify(search(query).slice(0, 3).map((r) => r.label)));
  }
});
