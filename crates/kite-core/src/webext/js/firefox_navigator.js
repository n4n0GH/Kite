// Extension pages tell their extension that it runs in Firefox. Kite's extensions are
// Firefox's builds, made for Firefox's API, which is the one Kite has; some look at the
// user agent to tell which browser's ways to go (Bitwarden took Chrome's: no sidebar,
// Chrome's storage workaround, its popup left open after filling a form).
//
// Only what scripts in the page read is changed. The engine's user agent is one for the
// whole profile: the pages' own requests and their workers still carry Chromium's.
function __kiteFirefoxNavigator(userAgent) {
  'use strict';
  const proto = Object.getPrototypeOf(navigator);
  const say = (name, value) => {
    try { Object.defineProperty(proto, name, { get() { return value; }, enumerable: true, configurable: true }); } catch (e) {}
  };
  say('userAgent', userAgent);
  say('appVersion', '5.0 (X11)');
  say('vendor', '');
  say('productSub', '20100101');
  say('oscpu', 'Linux x86_64');
  say('buildID', '20181001000000');
  // (Firefox has none; code asks it for the browser's brand before the user agent.)
  try { delete proto.userAgentData; } catch (e) {}
}
