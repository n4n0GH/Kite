// Run with: node --test crates/kite-core/src/webext/js/firefox_navigator.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(__dirname + '/firefox_navigator.js', 'utf8');
const FIREFOX = 'Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0';

// A navigator as Chromium's: its values come from getters on the prototype.
function chromium() {
  class Navigator {
    get userAgent() { return 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36'; }
    get appVersion() { return '5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36'; }
    get vendor() { return 'Google Inc.'; }
    get productSub() { return '20030107'; }
    get userAgentData() { return { brands: [{ brand: 'Chromium', version: '140' }] }; }
    get language() { return 'en-GB'; }
  }
  const sandbox = { navigator: new Navigator() };
  vm.createContext(sandbox);
  vm.runInContext(source, sandbox);
  return sandbox;
}

test('the page reads Firefox, and nothing of Chromium', () => {
  const page = chromium();
  vm.runInContext('__kiteFirefoxNavigator(' + JSON.stringify(FIREFOX) + ')', page);
  const n = page.navigator;
  assert.strictEqual(n.userAgent, FIREFOX);
  assert.strictEqual(n.appVersion, '5.0 (X11)');
  assert.strictEqual(n.vendor, '');
  assert.strictEqual(n.productSub, '20100101');
  assert.strictEqual(n.oscpu, 'Linux x86_64');
  assert.strictEqual(typeof n.buildID, 'string');
  assert.strictEqual(n.userAgentData, undefined);
  assert.ok(!('userAgentData' in n));
  // The rest is the engine's own.
  assert.strictEqual(n.language, 'en-GB');
});

test('what extensions look for', () => {
  const page = chromium();
  // Bitwarden's test, as shipped.
  const isFirefox = 'navigator.userAgent.indexOf(" Firefox/") !== -1 || navigator.userAgent.indexOf(" Gecko/") !== -1';
  const isChrome = 'navigator.userAgent.indexOf(" Chrome/") !== -1';
  assert.strictEqual(vm.runInContext(isFirefox, page), false);
  vm.runInContext('__kiteFirefoxNavigator(' + JSON.stringify(FIREFOX) + ')', page);
  assert.strictEqual(vm.runInContext(isFirefox, page), true);
  assert.strictEqual(vm.runInContext(isChrome, page), false);
});
