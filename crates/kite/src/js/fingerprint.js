// Fingerprint protection for a site ("privacy.antiFingerprinting", turned on
// per site in Site Settings). Runs in the page's own world before its
// scripts. Readings that differ between computers are blurred or made
// generic:
//   - canvas, WebGL and audio readouts get tiny changes: the same within a
//     site and session (pages reading twice see the same values), different
//     between sites and sessions, so they can't link visits;
//   - the WebGL graphics card name, CPU core count and memory size are
//     generic, and the screen's usable area is the whole screen.
// Patched functions are proxies of the originals, so they still read as
// native code. Not covered: workers, fonts.
(() => {
  'use strict';
  if (window.__kiteFingerprint) return;
  Object.defineProperty(window, '__kiteFingerprint', {value: true});

  const SEED = __KITE_SEED__ >>> 0;
  // A deterministic number for (seed, i): which pixels or samples change.
  const mix = i => {
    let h = (SEED ^ Math.imul(i + 0x9e3779b9, 0x85ebca6b)) >>> 0;
    h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35) >>> 0;
    return (h ^ (h >>> 16)) >>> 0;
  };

  // Proxies print as "function () { [native code] }", without the name:
  // Function.prototype.toString prints the original for each of ours.
  const originals = new WeakMap();
  const wrap = (orig, apply) => {
    const p = new Proxy(orig, {apply: (target, self, args) => apply(target, self, args)});
    originals.set(p, orig);
    return p;
  };
  const toString = Function.prototype.toString;
  Function.prototype.toString = wrap(toString, (target, self, args) => Reflect.apply(target, originals.get(self) || self, args));

  const patch = (proto, name, apply) => {
    const orig = proto && proto[name];
    if (typeof orig !== 'function') return;
    proto[name] = wrap(orig, apply);
  };
  const getter = (proto, name, get) => {
    const d = proto && Object.getOwnPropertyDescriptor(proto, name);
    if (!d || !d.get) return;
    Object.defineProperty(proto, name, {...d, get: wrap(d.get, get)});
  };

  // ---- pixels ---------------------------------------------------------------
  // About one pixel in 80 gets ±1 on one colour channel.
  const noisePixels = data => {
    for (let p = 0; p < data.length; p += 4) {
      const h = mix(p >> 2);
      if (h % 80 !== 0) continue;
      const c = p + ((h >>> 8) % 3);
      data[c] = data[c] + ((h & 0x10000) ? 1 : -1);
    }
    return data;
  };

  const getImageData = CanvasRenderingContext2D.prototype.getImageData;
  patch(CanvasRenderingContext2D.prototype, 'getImageData', (target, self, args) => {
    const img = Reflect.apply(target, self, args);
    noisePixels(img.data);
    return img;
  });

  // toDataURL / toBlob: the picture is exported from a blurred copy.
  const blurredCopy = canvas => {
    if (!canvas.width || !canvas.height) return canvas;
    let ctx;
    try { ctx = canvas.getContext('2d'); } catch (e) { return canvas; }
    if (!ctx) {
      // A WebGL canvas: copy it into a 2D canvas first.
      const c2 = document.createElement('canvas');
      c2.width = canvas.width; c2.height = canvas.height;
      c2.getContext('2d').drawImage(canvas, 0, 0);
      return blurredCopy(c2);
    }
    const copy = document.createElement('canvas');
    copy.width = canvas.width; copy.height = canvas.height;
    const img = Reflect.apply(getImageData, ctx, [0, 0, canvas.width, canvas.height]);
    noisePixels(img.data);
    copy.getContext('2d').putImageData(img, 0, 0);
    return copy;
  };
  patch(HTMLCanvasElement.prototype, 'toDataURL', (target, self, args) => Reflect.apply(target, blurredCopy(self), args));
  patch(HTMLCanvasElement.prototype, 'toBlob', (target, self, args) => Reflect.apply(target, blurredCopy(self), args));

  // ---- WebGL ----------------------------------------------------------------
  for (const GL of [window.WebGLRenderingContext, window.WebGL2RenderingContext]) {
    if (!GL) continue;
    patch(GL.prototype, 'getParameter', (target, self, args) => {
      // UNMASKED_VENDOR_WEBGL / UNMASKED_RENDERER_WEBGL: the names any
      // browser reports without the debug extension.
      if (args[0] === 0x9245) return 'WebKit';
      if (args[0] === 0x9246) return 'WebKit WebGL';
      return Reflect.apply(target, self, args);
    });
    patch(GL.prototype, 'readPixels', (target, self, args) => {
      const out = Reflect.apply(target, self, args);
      const data = args[6];
      if (data && typeof data.length === 'number' && (data instanceof Uint8Array || data instanceof Uint8ClampedArray)) noisePixels(data);
      return out;
    });
  }

  // ---- audio ----------------------------------------------------------------
  // Samples get a change far below hearing (about 1e-7), once per buffer.
  const noisedBuffers = new WeakSet();
  if (window.AudioBuffer) {
    patch(AudioBuffer.prototype, 'getChannelData', (target, self, args) => {
      const data = Reflect.apply(target, self, args);
      if (!noisedBuffers.has(data)) {
        noisedBuffers.add(data);
        for (let i = 0; i < data.length; i += 97) data[i] += ((mix(i) & 0xff) - 128) * 1e-9;
      }
      return data;
    });
  }
  if (window.AnalyserNode) {
    patch(AnalyserNode.prototype, 'getFloatFrequencyData', (target, self, args) => {
      const out = Reflect.apply(target, self, args);
      const data = args[0];
      if (data) for (let i = 0; i < data.length; i += 7) data[i] += ((mix(i) & 0xff) - 128) * 1e-4;
      return out;
    });
  }

  // ---- hardware -------------------------------------------------------------
  getter(Navigator.prototype, 'hardwareConcurrency', () => 4);
  getter(Navigator.prototype, 'deviceMemory', () => 8);
  // The usable screen area is the screen (no panel sizes or positions).
  getter(Screen.prototype, 'availWidth', (target, self) => self.width);
  getter(Screen.prototype, 'availHeight', (target, self) => self.height);
  getter(Screen.prototype, 'availLeft', () => 0);
  getter(Screen.prototype, 'availTop', () => 0);
  getter(Screen.prototype, 'colorDepth', () => 24);
  getter(Screen.prototype, 'pixelDepth', () => 24);
})();
