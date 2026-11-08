// Pages told the same story as their server, when Kite sends another browser's
// user agent: Google's sign-in (Settings → Privacy, "Sign in to Google") and
// sites given one of their own (Site Settings). The headers lose Chromium's
// client hints then (glue.cpp), so the pages lose navigator.userAgentData.
(() => {
  const set = (name, value) => Object.defineProperty(Navigator.prototype, name, { get() { return value; }, configurable: true, enumerable: true });
  const ua = __KITE_UA__;
  const gecko = /\bGecko\/\d/.test(ua) && !/like Gecko/.test(ua);
  const apple = /AppleWebKit\//.test(ua) && !/Chrome\/|Chromium\//.test(ua);
  set('userAgent', ua);
  // Firefox gives "5.0 (X11)", the others the user agent from after "Mozilla/".
  set('appVersion', gecko ? ua.slice(ua.indexOf('(')).replace(/^\(([^;)]*)[;)].*/, '5.0 ($1)').replace(/ NT [\d.]+/, '') : ua.replace(/^Mozilla\//, ''));
  set('vendor', gecko ? '' : apple ? 'Apple Computer, Inc.' : 'Google Inc.');
  const platform = /iPhone/.test(ua) ? 'iPhone' : /iPad/.test(ua) ? 'iPad' : /Android/.test(ua) ? 'Linux armv81'
    : /Windows/.test(ua) ? 'Win32' : /Macintosh/.test(ua) ? 'MacIntel' : /Linux/.test(ua) ? 'Linux x86_64' : null;
  if (platform) set('platform', platform);
  delete Navigator.prototype.userAgentData;
})();
