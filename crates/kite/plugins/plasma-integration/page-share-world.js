// navigator.share, which QtWebEngine doesn't have on Linux. In the page's own world,
// to be where the page looks; page-share.js decides, where the page can't interfere,
// and only says it's on if it's a setting that is, on a page that's allowed to.
(function () {
  'use strict';
  window.addEventListener('kite-plasma-share-on', () => {
    if ('share' in navigator) return;
    const waiting = new Map();
    let last = 0;
    window.addEventListener('kite-plasma-share-done', (e) => {
      const { id, error, message } = JSON.parse(e.detail);
      const promise = waiting.get(id);
      if (!promise) return;
      waiting.delete(id);
      if (!error) promise.resolve();
      else if (error === 'TypeError') promise.reject(new TypeError(message));
      else promise.reject(new DOMException(message, error));
    });
    const strings = (data) => {
      const out = {};
      for (const key of ['title', 'text', 'url']) if (data[key] !== undefined) out[key] = String(data[key]);
      return out;
    };
    Navigator.prototype.share = function share(data = {}) {
      return new Promise((resolve, reject) => {
        const id = ++last;
        waiting.set(id, { resolve, reject });
        window.dispatchEvent(new CustomEvent('kite-plasma-share', { detail: JSON.stringify({
          id, base: document.baseURI, files: !!(data.files && data.files.length), ...strings(data),
        }) }));
      });
    };
    Navigator.prototype.canShare = function canShare(data = {}) {
      if (data.files && data.files.length) return false;
      const s = strings(data);
      if (!s.title && !s.text && s.url === undefined) return false;
      if (s.url === undefined) return true;
      try { return !['about:', 'blob:', 'data:', 'file:', 'javascript:', 'ws:', 'wss:'].includes(new URL(s.url, document.baseURI).protocol); } catch (e) { return false; }
    };
  }, { once: true });
})();
