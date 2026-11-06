// What this frame is playing, for the desktop's media controls (PlasmaWindow.qml).
// page-world.js adds what only the page's own world can see. Reports say how things
// stand, whole: the browser works out what changed, and keeps the position running.
(function () {
  'use strict';
  if (globalThis.__kitePlasma || !KITE_PLUGIN_SETTINGS.mediaControls) return;
  const frame = Math.random().toString(36).slice(2);
  // Under this much is a notification sound, not something to control.
  const SHORTEST = 8;
  // The position it reports is worked out from there, so it should not drift far.
  const AGAIN_MS = 10000;
  const WATCHING = ['play', 'playing', 'pause', 'ended', 'waiting', 'seeked', 'ratechange', 'durationchange', 'loadedmetadata', 'volumechange', 'emptied'];

  let current = null;
  let session = { title: '', artist: '', album: '', art: '', actions: [] };
  let reported = false;
  let soon = 0;
  let again = 0;

  // A muted clip decorating the page isn't what the user is listening to.
  function worthControlling(el) {
    if (el.muted || el.volume === 0) return false;
    return el.duration === Infinity || el.duration >= SHORTEST;
  }
  function state() {
    const el = current;
    return {
      title: session.title, artist: session.artist, album: session.album, art: session.art || el.poster || '',
      src: el.currentSrc || '', playing: !el.paused && !el.ended,
      duration: Number.isFinite(el.duration) ? el.duration : null, position: el.currentTime || 0,
      rate: el.playbackRate, volume: el.volume, muted: el.muted,
      next: session.actions.includes('nexttrack'), previous: session.actions.includes('previoustrack'),
    };
  }
  function send() {
    soon = 0;
    if (current && !current.isConnected) current = Array.from(document.querySelectorAll('video, audio')).find((m) => !m.paused && worthControlling(m)) || null;
    clearTimeout(again);
    if (current) {
      reported = true;
      kitePluginSend({ cmd: 'media', frame, state: state() });
      if (!current.paused) again = setTimeout(send, AGAIN_MS);
    } else if (reported) {
      reported = false;
      kitePluginSend({ cmd: 'media', frame, state: null });
    }
  }
  // (Several of these come at once, as when something starts.)
  function schedule() {
    if (!soon) soon = setTimeout(send, 50);
  }
  function onMedia(e) {
    const el = e.target;
    if (!(el instanceof HTMLMediaElement)) return;
    if (el !== current) {
      if (el.paused || !worthControlling(el)) return;
      current = el;
    }
    schedule();
  }
  for (const name of WATCHING) document.addEventListener(name, onMedia, true);

  window.addEventListener('kite-plasma-session', (e) => {
    try { session = Object.assign(session, JSON.parse(e.detail)); } catch (error) { return; }
    if (current) schedule();
  });
  // Leaving the page. (From the back/forward cache it comes back as it was.)
  window.addEventListener('pagehide', () => {
    clearTimeout(soon);
    clearTimeout(again);
    soon = 0;
    if (reported) {
      reported = false;
      kitePluginSend({ cmd: 'media', frame, state: null });
    }
  });
  window.addEventListener('pageshow', (e) => { if (e.persisted && current) schedule(); });

  // The page's own way to do something, if it has one: its player keeps up that way.
  function pageDoes(action) {
    if (!session.actions.includes(action)) return false;
    window.dispatchEvent(new CustomEvent('kite-plasma-do', { detail: action }));
    return true;
  }
  const play = () => { if (!pageDoes('play')) current.play().catch(() => {}); };
  const pause = () => { if (!pageDoes('pause')) current.pause(); };
  const COMMANDS = {
    play, pause,
    playPause: () => { if (current.paused || current.ended) play(); else pause(); },
    stop: () => { if (!pageDoes('stop')) current.pause(); },
    next: () => pageDoes('nexttrack'),
    previous: () => pageDoes('previoustrack'),
    seek: (by) => { current.currentTime = Math.max(0, current.currentTime + by); },
    setPosition: (to) => { current.currentTime = to; },
    setVolume: (volume) => { current.muted = false; current.volume = volume; },
    setRate: (rate) => { current.playbackRate = rate; },
  };
  Object.defineProperty(globalThis, '__kitePlasma', { value: {
    command(forFrame, cmd, value) {
      if (forFrame === frame && current && Object.hasOwn(COMMANDS, cmd)) COMMANDS[cmd](value);
    },
  } });
})();
