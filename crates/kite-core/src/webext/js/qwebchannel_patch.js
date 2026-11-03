// Kite patch for Qt's qwebchannel.js client (loaded right after it).
//
// A page keeps one WebChannel transport across navigations, so replies and
// signals Qt sends for the previous document can reach the new document's
// client. Stock clients number requests from 0 and call whatever callback
// sits at a reply's id: an old reply either throws ("execCallbacks[id] is not
// a function") or, worse, is handed to an unrelated pending call of the new
// document; signals before init log "Unhandled signal". Here each client
// numbers its requests from a random offset (after the fixed init request 0)
// and ignores replies it didn't ask for and messages for objects it doesn't
// know yet.
(function () {
  'use strict';
  const Stock = QWebChannel;
  QWebChannel = function (transport, initCallback, converters) {
    const ch = new Stock(transport, initCallback, converters);
    ch.execId = 1 + Math.floor(Math.random() * 0x7fffffff) * 1024;
    const wrap = (name, accept) => {
      const stock = ch[name];
      if (typeof stock === 'function') ch[name] = (m) => { if (m && accept(m)) stock(m); };
    };
    wrap('handleResponse', (m) => typeof ch.execCallbacks[m.id] === 'function');
    wrap('handleSignal', (m) => !!ch.objects[m.object]);
    wrap('handlePropertyUpdate', () => Object.keys(ch.objects).length > 0);
    return ch;
  };
})();
