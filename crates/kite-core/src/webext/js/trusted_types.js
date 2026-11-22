// Trusted Types and content scripts.
//
// A page can require Trusted Types (`require-trusted-types-for 'script'`, as
// YouTube does): a plain string may then not become a script's address.
// Firefox and Chrome leave an extension's content scripts out of the page's
// policy. QtWebEngine doesn't: Kite's content-script worlds are held to it,
// and the usual way of putting an add-on's own script into a page,
//     script.src = browser.runtime.getURL('page.js')
// throws there.
//
// `__kiteTrustedScriptUrls(g, base)` makes that one case work, in this world
// only: its prototypes are its own, so the page's scripts see nothing of this
// and can't get at the policy. An address of the add-on's own files (`base`,
// "moz-extension://<uuid>/") that the engine refused as a string is given
// again through a policy made here.
//
// The policy is made when it is first needed, so on a page without Trusted
// Types nothing happens at all, and it makes nothing but those addresses.
// Where the page also says which policies there may be (`trusted-types …`),
// none can be made, and the add-on gets the engine's error as before.
function __kiteTrustedScriptUrls(g, base) {
  'use strict';
  const tt = g.trustedTypes;
  const Script = g.HTMLScriptElement, Element = g.Element;
  if (!tt || typeof tt.createPolicy !== 'function' || !Script || !Element) return;
  const src = Object.getOwnPropertyDescriptor(Script.prototype, 'src');
  const setAttribute = Element.prototype.setAttribute;
  if (!src || !src.set || typeof setAttribute !== 'function') return;

  // The address as text, if it is one of the add-on's own files.
  const own = (v) => {
    const s = typeof v === 'string' ? v : (g.URL && v instanceof g.URL) ? v.href : null;
    return s !== null && s.startsWith(base) ? s : null;
  };
  let policy = null;
  // The engine has refused such a string here: the policy at once from now on
  // (each refusal is an error in the log, and a report to the site).
  let refused = false;
  // No policy may be made on this page.
  let unavailable = false;
  const trusted = (s) => {
    if (!policy) {
      policy = tt.createPolicy('content-script', {
        createScriptURL: (u) => { if (own(u) === null) throw new TypeError('Not a file of this extension'); return u; },
      });
    }
    return policy.createScriptURL(s);
  };
  // `set(value)`: the engine's own way of setting it.
  const give = (set, v) => {
    const s = unavailable ? null : own(v);
    if (s === null) return set(v);
    if (!refused) {
      try { return set(v); } catch (e) {
        if (!e || e.name !== 'TypeError') throw e;
        refused = true;
      }
    }
    let t;
    try { t = trusted(s); } catch (e) { unavailable = true; return set(v); }
    return set(t);
  };

  Object.defineProperty(Script.prototype, 'src', {
    configurable: true, enumerable: src.enumerable, get: src.get,
    set(v) { give((x) => src.set.call(this, x), v); },
  });
  Element.prototype.setAttribute = function (name, value) {
    if (arguments.length >= 2 && this instanceof Script && typeof name === 'string' && name.toLowerCase() === 'src') {
      return give((x) => setAttribute.call(this, name, x), value);
    }
    return setAttribute.apply(this, arguments);
  };
}
