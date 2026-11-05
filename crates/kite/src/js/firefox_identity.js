// Google's sign-in refuses browsers it doesn't know (Settings → Privacy, "Sign in to
// Google"; ffi.rs sends the same as this in the headers). Pages of that one site are
// told the same story as its server: Firefox, which has no client hints either.
(() => {
  const set = (name, value) => Object.defineProperty(Navigator.prototype, name, { get() { return value; }, configurable: true, enumerable: true });
  const ua = '__KITE_UA__';
  set('userAgent', ua);
  set('appVersion', ua.slice(ua.indexOf('(')).replace(/^\((X11); [^)]*\).*/, '5.0 ($1)'));
  set('vendor', '');
  delete Navigator.prototype.userAgentData;
})();
