// Web MIDI for web pages (navigator.requestMIDIAccess).
//
// QtWebEngine has no permission type for MIDI and refuses every request without asking, so
// Kite implements the API: this main-world script asks Kite through DOM events relayed by
// the bridge world (bridge.js); Kite prompts the user and talks to the devices
// (kite_core::midi). What's checked is checked there: this script runs among the page's own,
// so it's only here to behave as the standard says. Top-level documents only (Kite's bridge
// lives there).
(function () {
  'use strict';
  if (window !== top || !isSecureContext || !navigator.requestMIDIAccess) return;

  const send = (name, detail) => document.dispatchEvent(new CustomEvent(name, { detail: JSON.stringify(detail) }));
  const op = (detail) => send('kite-midi-op', detail);
  const listen = (name, handler) => document.addEventListener(name, (e) => {
    let detail; try { detail = JSON.parse(e.detail); } catch (x) { return; }
    handler(detail);
  });

  // Is this whole MIDI messages, one after the other?
  function validMessages(data, sysexAllowed) {
    let at = 0;
    while (at < data.length) {
      const status = data[at++];
      if (status < 0x80) return false;
      if (status === 0xF0) {
        if (!sysexAllowed) return false;
        for (;;) {
          if (at >= data.length) return false;
          const b = data[at];
          if (b === 0xF7) break;
          if (b >= 0x80 && b < 0xF8) return false;
          at++;
        }
        at++;
        continue;
      }
      let needed;
      if ((status >= 0x80 && status <= 0xBF) || (status >= 0xE0 && status <= 0xEF) || status === 0xF2) needed = 2;
      else if ((status >= 0xC0 && status <= 0xDF) || status === 0xF1 || status === 0xF3) needed = 1;
      else if (status === 0xF6 || status >= 0xF8) needed = 0;
      else return false;
      for (let i = 0; i < needed; i++) {
        if (at >= data.length || data[at] >= 0x80) return false;
        at++;
      }
    }
    return true;
  }

  // A port stays open while any access has it open.
  const openCount = new Map();
  function holdOpen(id) {
    const n = openCount.get(id) || 0;
    openCount.set(id, n + 1);
    if (n === 0) op({ op: 'open', id });
  }
  function letClose(id) {
    const n = openCount.get(id) || 0;
    if (n <= 1) {
      openCount.delete(id);
      if (n === 1) op({ op: 'close', id });
    } else {
      openCount.set(id, n - 1);
    }
  }

  const accesses = new Set();

  class MIDIPort extends EventTarget {
    #info; #access; #state = 'connected'; #connection = 'closed'; #onstatechange = null;
    constructor(info, access) { super(); this.#info = info; this.#access = access; }
    get id() { return this.#info.id; }
    get manufacturer() { return ''; }
    get name() { return this.#info.name; }
    get type() { return this.#info.type; }
    get version() { return ''; }
    get state() { return this.#state; }
    get connection() { return this.#connection; }
    get onstatechange() { return this.#onstatechange; }
    set onstatechange(fn) {
      if (this.#onstatechange) this.removeEventListener('statechange', this.#onstatechange);
      this.#onstatechange = typeof fn === 'function' ? fn : null;
      if (this.#onstatechange) this.addEventListener('statechange', this.#onstatechange);
    }
    open() {
      this._openNow();
      return Promise.resolve(this);
    }
    close() {
      if (this.#connection !== 'closed') {
        this.#connection = 'closed';
        letClose(this.id);
        this._changed();
      }
      return Promise.resolve(this);
    }
    _openNow() {
      if (this.#connection === 'open' || this.#connection === 'pending') return;
      this.#connection = this.#state === 'connected' ? 'open' : 'pending';
      holdOpen(this.id);
      this._changed();
    }
    _setState(state) {
      if (state === this.#state) return;
      this.#state = state;
      if (state === 'disconnected' && this.#connection === 'open') this.#connection = 'pending';
      else if (state === 'connected' && this.#connection === 'pending') this.#connection = 'open';
      this._changed();
    }
    _changed() { this.#access._portChanged(this); }
    _fire() {
      const event = new Event('statechange');
      Object.defineProperty(event, 'port', { value: this, enumerable: true });
      this.dispatchEvent(event);
    }
    get _sysex() { return this.#access.sysexEnabled; }
    get [Symbol.toStringTag]() { return 'MIDIPort'; }
  }

  class MIDIInput extends MIDIPort {
    #onmidimessage = null;
    get onmidimessage() { return this.#onmidimessage; }
    set onmidimessage(fn) {
      if (this.#onmidimessage) super.removeEventListener('midimessage', this.#onmidimessage);
      this.#onmidimessage = typeof fn === 'function' ? fn : null;
      if (this.#onmidimessage) {
        super.addEventListener('midimessage', this.#onmidimessage);
        this._openNow();
      }
    }
    // (Listening is what opens an input, however it's done.)
    addEventListener(type, listener, options) {
      super.addEventListener(type, listener, options);
      if (type === 'midimessage' && listener) this._openNow();
    }
    _deliver(data) {
      if (this.connection !== 'open') return;
      const bytes = Uint8Array.from(data);
      if (bytes[0] === 0xF0 && !this._sysex) return;
      let event;
      if (typeof MIDIMessageEvent === 'function') {
        event = new MIDIMessageEvent('midimessage', { data: bytes });
      } else {
        event = new Event('midimessage');
        Object.defineProperty(event, 'data', { value: bytes, enumerable: true });
      }
      this.dispatchEvent(event);
    }
    get [Symbol.toStringTag]() { return 'MIDIInput'; }
  }

  class MIDIOutput extends MIDIPort {
    #timers = new Set();
    send(data, timestamp) {
      const bytes = Array.from(data, (b) => {
        const n = Number(b);
        if (!Number.isInteger(n) || n < 0 || n > 255) throw new TypeError('MIDI data must be bytes');
        return n;
      });
      if (this.state === 'disconnected') throw new DOMException('The port is disconnected', 'InvalidStateError');
      if (!validMessages(bytes, true)) throw new TypeError('The data is not valid MIDI messages');
      if (!validMessages(bytes, this._sysex)) throw new DOMException('SysEx messages need access with sysex: true', 'InvalidAccessError');
      this._openNow();
      const now = () => op({ op: 'send', id: this.id, data: bytes });
      const delay = timestamp === undefined ? 0 : Number(timestamp) - performance.now();
      if (delay > 1) {
        const timer = setTimeout(() => { this.#timers.delete(timer); now(); }, delay);
        this.#timers.add(timer);
      } else {
        now();
      }
    }
    clear() {
      for (const timer of this.#timers) clearTimeout(timer);
      this.#timers.clear();
    }
    get [Symbol.toStringTag]() { return 'MIDIOutput'; }
  }

  // Read-only, and otherwise like a Map.
  class PortMap {
    #map;
    constructor(map) { this.#map = map; }
    get size() { return this.#map.size; }
    get(key) { return this.#map.get(key); }
    has(key) { return this.#map.has(key); }
    keys() { return this.#map.keys(); }
    values() { return this.#map.values(); }
    entries() { return this.#map.entries(); }
    forEach(fn, thisArg) { this.#map.forEach((value, key) => fn.call(thisArg, value, key, this)); }
    [Symbol.iterator]() { return this.#map.entries(); }
  }
  class MIDIInputMap extends PortMap { get [Symbol.toStringTag]() { return 'MIDIInputMap'; } }
  class MIDIOutputMap extends PortMap { get [Symbol.toStringTag]() { return 'MIDIOutputMap'; } }

  class MIDIAccess extends EventTarget {
    #inputs = new Map(); #outputs = new Map(); #sysex; #onstatechange = null;
    #inputMap = new MIDIInputMap(this.#inputs); #outputMap = new MIDIOutputMap(this.#outputs);
    constructor(ports, sysex) {
      super();
      this.#sysex = sysex;
      this._ports(ports);
    }
    get inputs() { return this.#inputMap; }
    get outputs() { return this.#outputMap; }
    get sysexEnabled() { return this.#sysex; }
    get onstatechange() { return this.#onstatechange; }
    set onstatechange(fn) {
      if (this.#onstatechange) this.removeEventListener('statechange', this.#onstatechange);
      this.#onstatechange = typeof fn === 'function' ? fn : null;
      if (this.#onstatechange) this.addEventListener('statechange', this.#onstatechange);
    }
    _ports(ports) {
      const present = new Set(ports.map((p) => p.id));
      for (const info of ports) {
        const map = info.type === 'input' ? this.#inputs : this.#outputs;
        const known = map.get(info.id);
        if (known) {
          known._setState('connected');
        } else {
          const port = info.type === 'input' ? new MIDIInput(info, this) : new MIDIOutput(info, this);
          map.set(info.id, port);
          this._portChanged(port);
        }
      }
      // (A port that's gone stays listed: the standard has it show up as disconnected.)
      for (const map of [this.#inputs, this.#outputs]) {
        for (const [id, port] of map) if (!present.has(id)) port._setState('disconnected');
      }
    }
    _portChanged(port) {
      port._fire();
      const event = new Event('statechange');
      Object.defineProperty(event, 'port', { value: port, enumerable: true });
      this.dispatchEvent(event);
    }
    _message(id, data) {
      const port = this.#inputs.get(id);
      if (port) port._deliver(data);
    }
    get [Symbol.toStringTag]() { return 'MIDIAccess'; }
  }

  let nextId = 1;
  const pending = new Map();
  const stateQueries = new Map();

  listen('kite-midi-answer', (a) => {
    const p = pending.get(a.id);
    if (!p) return;
    pending.delete(a.id);
    if (a.error) { p.reject(new DOMException(a.error, 'NotSupportedError')); return; }
    if (!a.granted) { p.reject(new DOMException('Permission to use MIDI devices was denied', 'NotAllowedError')); return; }
    const access = new MIDIAccess(a.ports || [], p.sysex);
    accesses.add(access);
    p.resolve(access);
  });
  listen('kite-midi-event', (e) => {
    if (e.event === 'ports') for (const access of accesses) access._ports(e.ports);
    else if (e.event === 'message') for (const access of accesses) access._message(e.port, e.data);
  });
  listen('kite-midi-state', (a) => {
    const resolve = stateQueries.get(a.id);
    if (resolve) { stateQueries.delete(a.id); resolve(a.state); }
  });

  Object.defineProperty(Navigator.prototype, 'requestMIDIAccess', {
    configurable: true,
    writable: true,
    value: function requestMIDIAccess(options) {
      const sysex = !!(options && options.sysex);
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject, sysex });
        send('kite-midi-request', { id, sysex });
      });
    },
  });

  // navigator.permissions would say it's denied, which sites believe.
  class MidiPermissionStatus extends EventTarget {
    #state; #onchange = null;
    constructor(state) { super(); this.#state = state; }
    get name() { return 'midi'; }
    get state() { return this.#state; }
    get status() { return this.#state; }
    get onchange() { return this.#onchange; }
    set onchange(fn) {
      if (this.#onchange) this.removeEventListener('change', this.#onchange);
      this.#onchange = typeof fn === 'function' ? fn : null;
      if (this.#onchange) this.addEventListener('change', this.#onchange);
    }
    get [Symbol.toStringTag]() { return 'PermissionStatus'; }
  }
  if (typeof Permissions !== 'undefined' && Permissions.prototype.query) {
    const query = Permissions.prototype.query;
    Object.defineProperty(Permissions.prototype, 'query', {
      configurable: true,
      writable: true,
      value: function (descriptor) {
        if (!descriptor || descriptor.name !== 'midi') return query.call(this, descriptor);
        return new Promise((resolve) => {
          const id = nextId++;
          stateQueries.set(id, (state) => resolve(new MidiPermissionStatus(state)));
          send('kite-midi-query', { id, sysex: !!descriptor.sysex });
        });
      },
    });
  }

  for (const c of [MIDIAccess, MIDIPort, MIDIInput, MIDIOutput, MIDIInputMap, MIDIOutputMap]) {
    Object.defineProperty(window, c.name, { configurable: true, writable: true, value: c });
  }

  // Devices aren't kept open for a page that's gone, unless it could come back as it was.
  addEventListener('pagehide', (e) => { if (!e.persisted) op({ op: 'stop' }); });
})();
