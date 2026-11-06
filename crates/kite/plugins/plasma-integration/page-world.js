// In the page's own world, because that's where navigator.mediaSession is: what the
// page says about what it's playing, for page.js, and calling back what it wants done.
// Also Audio objects never put in the document, which page.js couldn't otherwise see.
// A page can imitate all of this, but only to mislead itself: page.js treats it as text.
(function () {
  'use strict';
  const ACTIONS = ['play', 'pause', 'stop', 'nexttrack', 'previoustrack'];
  const handlers = new Map();
  let metadata = null;

  // The biggest picture on offer.
  function art(list) {
    let best = '', size = -1;
    for (const a of Array.from(list || [])) {
      const n = parseInt(String(a.sizes || '')) || 0;
      if (n > size && a.src) { size = n; best = String(a.src); }
    }
    try { return best ? new URL(best, location.href).href : ''; } catch (e) { return ''; }
  }
  function report() {
    const m = metadata || {};
    window.dispatchEvent(new CustomEvent('kite-plasma-session', { detail: JSON.stringify({
      title: String(m.title || ''), artist: String(m.artist || ''), album: String(m.album || ''), art: art(m.artwork),
      actions: ACTIONS.filter((a) => handlers.has(a)),
    }) }));
  }

  // QtWebEngine may not provide it. Sites only look for it to tell it things, so
  // here it can be as simple as this.
  if (!('mediaSession' in navigator)) {
    window.MediaSession = class MediaSession {};
    Object.assign(window.MediaSession.prototype, { playbackState: 'none', setPositionState() {}, setCameraActive() {}, setMicrophoneActive() {}, setActionHandler() {} });
    Object.defineProperty(window.MediaSession.prototype, 'metadata', { configurable: true, get() { return null; }, set() {} });
    const session = new window.MediaSession();
    Object.defineProperty(navigator, 'mediaSession', { configurable: true, get: () => session });
  }
  if (!window.MediaMetadata) {
    window.MediaMetadata = class MediaMetadata {
      constructor(m) { Object.assign(this, { title: '', artist: '', album: '', artwork: [] }, m); }
    };
  }

  const proto = window.MediaSession.prototype;
  const given = Object.getOwnPropertyDescriptor(proto, 'metadata');
  Object.defineProperty(proto, 'metadata', {
    configurable: true, enumerable: true,
    get() { return metadata; },
    set(value) {
      if (given && given.set) given.set.call(this, value);
      metadata = value;
      report();
    },
  });
  const setHandler = proto.setActionHandler;
  proto.setActionHandler = function (action, handler) {
    const result = setHandler.apply(this, arguments);
    if (handler) handlers.set(action, handler); else handlers.delete(action);
    if (ACTIONS.includes(action)) report();
    return result;
  };
  // Changed after being set.
  if (window.MediaMetadata.prototype) {
    for (const key of ['title', 'artist', 'album', 'artwork']) {
      const d = Object.getOwnPropertyDescriptor(window.MediaMetadata.prototype, key);
      if (!d || !d.set) continue;
      Object.defineProperty(window.MediaMetadata.prototype, key, Object.assign({}, d, {
        set(value) { d.set.call(this, value); if (this === metadata) report(); },
      }));
    }
  }

  window.addEventListener('kite-plasma-do', (e) => {
    const handler = handlers.get(e.detail);
    if (handler) try { handler({ action: e.detail }); } catch (error) { console.error(error); }
  });

  // Keeps an element that plays outside the document where page.js can see it.
  const play = HTMLMediaElement.prototype.play;
  let holder = null;
  HTMLMediaElement.prototype.play = function () {
    if (!this.isConnected && document.documentElement) {
      if (!holder || !holder.isConnected) {
        holder = document.createElement('kite-plasma-media');
        holder.hidden = true;
        document.documentElement.append(holder);
      }
      holder.append(this);
    }
    return play.apply(this, arguments);
  };
})();
