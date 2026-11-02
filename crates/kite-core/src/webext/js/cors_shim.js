// Extension pages in Firefox may make cross-origin requests to hosts they
// have permission for, without CORS. Chromium enforces CORS on our custom
// scheme, so cross-origin fetch/XHR from extension pages are performed by
// Kite natively (permission-checked in the runtime).
function __kiteInstallCorsShim(call) {
  'use strict';
  const crossOrigin = (u) => { try { const x = new URL(u, location.href); return /^https?:$/.test(x.protocol) && x.origin !== location.origin; } catch (e) { return false; } };
  const b64 = (buf) => { let s = ''; const a = new Uint8Array(buf); for (let i = 0; i < a.length; i += 0x8000) s += String.fromCharCode.apply(null, a.subarray(i, i + 0x8000)); return btoa(s); };
  const unb64 = (s) => { const bin = atob(s); const a = new Uint8Array(bin.length); for (let i = 0; i < bin.length; i++) a[i] = bin.charCodeAt(i); return a; };
  async function native(url, method, headers, body) {
    let bodyB64 = null;
    if (body !== undefined && body !== null) bodyB64 = b64(await new Response(body).arrayBuffer());
    return call('_.fetch', { url: String(url), method: method || 'GET', headers, body: bodyB64 });
  }
  const origFetch = globalThis.fetch;
  globalThis.fetch = async function (input, init) {
    const req = input instanceof Request ? input : null;
    const url = req ? req.url : String(input);
    if (!crossOrigin(url)) return origFetch.apply(this, arguments);
    init = init || {};
    const method = (init.method || (req && req.method) || 'GET').toUpperCase();
    const hdrs = []; new Headers(init.headers || (req ? req.headers : undefined)).forEach((v, k) => hdrs.push([k, v]));
    let body = init.body; if (body === undefined && req && method !== 'GET' && method !== 'HEAD') body = await req.arrayBuffer();
    let r;
    try { r = await native(new URL(url, location.href).href, method, hdrs, body); }
    catch (e) { throw new TypeError('NetworkError when attempting to fetch resource.'); }
    const resp = new Response(r.status === 204 || r.status === 304 || method === 'HEAD' ? null : unb64(r.body), { status: r.status, statusText: r.statusText || '', headers: r.headers });
    Object.defineProperty(resp, 'url', { value: r.url });
    return resp;
  };
  const OrigXHR = globalThis.XMLHttpRequest;
  class KiteXHR extends EventTarget {
    constructor() { super(); this._x = null; this._h = []; this.readyState = 0; this.status = 0; this.statusText = ''; this.response = null; this.responseText = ''; this.responseType = ''; this.responseURL = ''; this.timeout = 0; this.withCredentials = false; this.upload = new EventTarget(); this._rh = []; }
    open(method, url, async, user, pw) {
      this._m = String(method).toUpperCase(); this._u = new URL(url, location.href).href;
      if (!crossOrigin(this._u)) { this._x = new OrigXHR(); this._x.open(method, url, async !== false, user, pw); this._wire(); }
      this.readyState = 1; this._emit('readystatechange');
    }
    _wire() {
      const x = this._x;
      for (const ev of ['load', 'error', 'abort', 'timeout', 'progress', 'loadstart', 'loadend', 'readystatechange']) {
        x.addEventListener(ev, () => {
          for (const k of ['readyState', 'status', 'statusText', 'responseURL']) this[k] = x[k];
          if (x.readyState === 4 || ev === 'load') { try { this.responseText = (x.responseType === '' || x.responseType === 'text') ? x.responseText : ''; } catch (e) {} this.response = x.response; }
          this._emit(ev);
        });
      }
    }
    setRequestHeader(k, v) { if (this._x) this._x.setRequestHeader(k, v); else this._h.push([k, v]); }
    getResponseHeader(k) { if (this._x) return this._x.getResponseHeader(k); const f = this._rh.find((h) => h[0].toLowerCase() === String(k).toLowerCase()); return f ? f[1] : null; }
    getAllResponseHeaders() { return this._x ? this._x.getAllResponseHeaders() : this._rh.map((h) => h[0] + ': ' + h[1]).join('\r\n'); }
    overrideMimeType(m) { if (this._x) this._x.overrideMimeType(m); }
    abort() { if (this._x) this._x.abort(); else { this._aborted = true; this._emit('abort'); } }
    send(body) {
      if (this._x) { this._x.responseType = this.responseType; this._x.timeout = this.timeout; this._x.withCredentials = this.withCredentials; return this._x.send(body); }
      this._emit('loadstart');
      native(this._u, this._m, this._h, body).then((r) => {
        if (this._aborted) return;
        const bytes = unb64(r.body);
        this._rh = r.headers; this.status = r.status; this.statusText = r.statusText || ''; this.responseURL = r.url;
        const text = () => new TextDecoder().decode(bytes);
        switch (this.responseType) {
          case 'arraybuffer': this.response = bytes.buffer; break;
          case 'blob': this.response = new Blob([bytes], { type: this.getResponseHeader('content-type') || '' }); break;
          case 'json': try { this.response = JSON.parse(text()); } catch (e) { this.response = null; } break;
          case 'document': this.response = new DOMParser().parseFromString(text(), 'text/html'); break;
          default: this.responseText = text(); this.response = this.responseText;
        }
        this.readyState = 4; this._emit('readystatechange'); this._emit('load'); this._emit('loadend');
      }, () => { if (this._aborted) return; this.readyState = 4; this.status = 0; this._emit('readystatechange'); this._emit('error'); this._emit('loadend'); });
    }
    _emit(type) { const e = new Event(type); const h = this['on' + type]; if (typeof h === 'function') { try { h.call(this, e); } catch (x) { console.error(x); } } this.dispatchEvent(e); }
  }
  KiteXHR.UNSENT = 0; KiteXHR.OPENED = 1; KiteXHR.HEADERS_RECEIVED = 2; KiteXHR.LOADING = 3; KiteXHR.DONE = 4;
  globalThis.XMLHttpRequest = KiteXHR;
}
