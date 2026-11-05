// Forms in frames: forms.js, given a stand-in for the top frame's web channel,
// which Qt only offers there.
//
// Up, messages go to the browser as console messages from this isolated world,
// which page scripts can neither see nor (lacking the secret) imitate. Down, the
// browser runs __kiteFrameDown in this very frame through Qt's frame object, so
// nothing passes through anything the page can read. It gives the origin it
// means, in case the frame has moved on.
(function () {
  const SECRET = '__KITE_FRAME_SECRET__';
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  const id = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  const send = (m, ...a) => console.debug(SECRET + JSON.stringify({ f: id, m, a }));
  const handlers = {};
  const signal = (name) => ({ connect: (fn) => { handlers[name] = fn; } });
  globalThis.__kiteFrameId = id;
  globalThis.__kiteFrameDown = (name, arg, origin) => {
    if (location.origin === origin && handlers[name]) handlers[name](arg);
  };
  const k = {
    formSubmitted: (json) => send('formSubmitted', json),
    loginFormFound: (empty) => send('loginFormFound', empty),
    autofillFocus: (json) => send('autofillFocus', json),
    autofillBlur: () => send('autofillBlur'),
    autofillKey: (key) => send('autofillKey', key),
    autofillSubmit: (json) => send('autofillSubmit', json),
    formDirty: (dirty) => send('formDirty', dirty),
    fillLogin: signal('fillLogin'),
    fillAddress: signal('fillAddress'),
    fillPassword: signal('fillPassword'),
    autofillMenu: signal('autofillMenu'),
  };
  globalThis.__kiteFormsReady(k);
  globalThis.__kiteFillReady(k);
})();
