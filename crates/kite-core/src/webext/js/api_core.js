// Kite WebExtension API (browser.* / chrome.*), shared by extension pages and
// content-script worlds. `__kiteMakeAPI(cfg, transport)` returns
// {browser, chrome, dispatch}. The transport sends calls to the Rust runtime:
//   transport.call(method, argsArray) -> Promise
// and the runtime's events/messages are fed into `dispatch(payload)`.
function __kiteMakeAPI(cfg, transport) {
  'use strict';
  const isContent = cfg.kind === 'content';
  const base = 'moz-extension://' + cfg.uuid + '/';
  let lastError = null;

  const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v, (k, x) => (typeof x === 'bigint' ? Number(x) : x))));
  const call = (method, ...args) => {
    // Trailing undefined args are dropped like in Firefox's schema normalization.
    while (args.length && args[args.length - 1] === undefined) args.pop();
    let payload;
    try { payload = clone(args.map((a) => (a === undefined ? null : a))); }
    catch (e) { return Promise.reject(new Error('Argument is not serializable: ' + e.message)); }
    return transport.call(method, payload).then((v) => {
      if (v && typeof v === 'object' && '__kiteError' in v) throw new Error(v.__kiteError);
      return v === null ? undefined : v;
    });
  };

  // ---- events -----------------------------------------------------------
  const registry = new Map();
  class KiteEvent {
    constructor(name, hooks = {}) { this._name = name; this._l = []; this._hooks = hooks; registry.set(name, this); }
    addListener(cb, ...extra) {
      if (typeof cb !== 'function') throw new TypeError('Listener must be a function');
      if (this.hasListener(cb)) return;
      this._l.push({ cb, extra });
      if (this._hooks.add) this._hooks.add(cb, extra, this._l.length);
    }
    removeListener(cb) {
      const i = this._l.findIndex((x) => x.cb === cb);
      if (i >= 0) { const [{ extra }] = this._l.splice(i, 1); if (this._hooks.remove) this._hooks.remove(cb, this._l.length, extra); }
    }
    hasListener(cb) { return this._l.some((x) => x.cb === cb); }
    hasListeners() { return this._l.length > 0; }
    _fire(args) {
      const results = [];
      for (const { cb, extra } of this._l.slice()) {
        if (this._hooks.filter && !this._hooks.filter(args, extra)) continue;
        try { results.push(cb(...clone(args))); } catch (e) { console.error('Error in ' + this._name + ' listener:', e); }
      }
      return results;
    }
  }
  const ev = (name, hooks) => new KiteEvent(name, hooks);

  // ---- messaging --------------------------------------------------------
  const ports = new Map();
  class Port {
    constructor(id, name, sender) {
      this._id = id; this.name = name; this.error = null;
      if (sender) this.sender = sender;
      this.onMessage = new KiteEvent('port.onMessage');
      this.onDisconnect = new KiteEvent('port.onDisconnect');
      this._open = true;
      ports.set(id, this);
    }
    postMessage(msg) {
      if (!this._open) throw new Error('Attempt to postMessage on disconnected port');
      call('_.portPost', this._id, msg).catch(() => {});
    }
    disconnect() {
      if (!this._open) return;
      this._open = false; ports.delete(this._id);
      call('_.portDisconnect', this._id).catch(() => {});
    }
  }
  const newPortId = () => cfg.uuid.slice(0, 8) + '-' + Math.random().toString(36).slice(2) + Date.now().toString(36);

  function handleMessage(evt, p) {
    let done = false;
    const respond = (v, err) => {
      if (done) return; done = true;
      call('_.respond', p.msgId, err ? { __kiteError: String(err && err.message || err) } : (v === undefined ? null : v), true).catch(() => {});
    };
    let pending = false;
    const results = [];
    for (const { cb } of evt._l.slice()) {
      try { results.push(cb(clone(p.message), clone(p.sender), (v) => respond(v))); } catch (e) { console.error('Error in runtime.onMessage listener:', e); }
    }
    for (const r of results) {
      if (r === true) pending = true;
      else if (r && typeof r.then === 'function') { pending = true; r.then((v) => respond(v), (e) => respond(undefined, e)); }
    }
    if (!pending && !done) { done = true; call('_.respond', p.msgId, null, false).catch(() => {}); }
  }

  // ---- event filters ----------------------------------------------------
  const tabsUpdatedFilter = (args, extra) => {
    const f = extra[0]; if (!f) return true;
    const [tabId, changes, tab] = args;
    if (f.tabId !== undefined && f.tabId !== tabId) return false;
    if (f.windowId !== undefined && f.windowId !== tab.windowId) return false;
    if (f.properties && !f.properties.some((p) => p in changes || (p === 'favIconUrl' && 'favIconUrl' in changes))) return false;
    if (f.urls && !(tab.url && f.urls.some((p) => matchPattern(p, tab.url)))) return false;
    return true;
  };
  const navFilter = (args, extra) => {
    const f = extra[0]; if (!f || !f.url) return true;
    let u; try { u = new URL(args[0].url); } catch (e) { return false; }
    return f.url.some((c) => Object.entries(c).every(([k, v]) => {
      switch (k) {
        case 'hostContains': return u.hostname.includes(v);
        case 'hostEquals': return u.hostname === v;
        case 'hostPrefix': return u.hostname.startsWith(v);
        case 'hostSuffix': return u.hostname.endsWith(v);
        case 'pathContains': return u.pathname.includes(v);
        case 'pathEquals': return u.pathname === v;
        case 'pathPrefix': return u.pathname.startsWith(v);
        case 'pathSuffix': return u.pathname.endsWith(v);
        case 'queryContains': return u.search.includes(v);
        case 'urlContains': return u.href.includes(v);
        case 'urlEquals': return u.href === v;
        case 'urlPrefix': return u.href.startsWith(v);
        case 'urlSuffix': return u.href.endsWith(v);
        case 'urlMatches': return new RegExp(v).test(u.href);
        case 'originAndPathMatches': return new RegExp(v).test(u.origin + u.pathname);
        case 'schemes': return v.includes(u.protocol.slice(0, -1));
        case 'ports': return v.some((p) => (Array.isArray(p) ? +u.port >= p[0] && +u.port <= p[1] : +u.port === p));
        default: return true;
      }
    }));
  };
  function matchPattern(p, url) {
    if (p === '<all_urls>') return /^(https?|wss?|ftp|file|data):/.test(url);
    const m = /^(\*|[a-z-]+):\/\/([^/]*)(\/.*)?$/.exec(p); if (!m) return false;
    const esc = (s) => s.replace(/[.+?^${}()|[\]\\]/g, '\\$&');
    const scheme = m[1] === '*' ? '(https?|wss?)' : esc(m[1]);
    const host = m[2] === '*' ? '[^/]*' : m[2].startsWith('*.') ? '([^/]*\\.)?' + esc(m[2].slice(2)) + '(:\\d+)?' : esc(m[2]) + '(:\\d+)?';
    const path = (m[3] || '/').split('*').map(esc).join('.*');
    return new RegExp('^' + scheme + '://' + host + path + '$').test(url.split('#')[0]);
  }
  // webRequest. Non-blocking listeners get events through the transport.
  // Blocking ones (extension pages only) are asked synchronously by the
  // network interceptor over a WebSocket to Kite's blocking hub (see
  // webext/blocking.rs) and answer with {cancel} / {redirectUrl} /
  // {requestHeaders}, possibly through a promise.
  const isBlocking = (extra) => !isContent && !!cfg.blockingPort && (extra[1] || []).includes('blocking');
  const webRequestMatches = (f, d) => {
    if (!f) return true;
    if (f.urls && !f.urls.some((p) => matchPattern(p, d.url))) return false;
    if (f.types && f.types.length && !f.types.includes(d.type)) return false;
    if (f.tabId !== undefined && f.tabId !== d.tabId) return false;
    if (f.windowId !== undefined && d.windowId !== undefined && f.windowId !== d.windowId) return false;
    return true;
  };
  let blockingSocket = null, blockingListeners = 0;
  function answerBlocking(q) {
    const e = registry.get('webRequest.' + q.event);
    const results = [];
    for (const { cb, extra } of e ? e._l.slice() : []) {
      if (!isBlocking(extra) || !webRequestMatches(extra[0], q.details)) continue;
      try { results.push(cb(clone(q.details))); } catch (err) { console.error('Error in webRequest.' + q.event + ' listener:', err); }
    }
    const merge = (rs) => {
      const out = {};
      for (const r of rs) {
        if (!r || typeof r !== 'object') continue;
        if (r.cancel) out.cancel = true;
        if (r.redirectUrl && !out.redirectUrl) out.redirectUrl = String(r.redirectUrl);
        if (r.requestHeaders) out.requestHeaders = r.requestHeaders;
        if (r.responseHeaders) out.responseHeaders = r.responseHeaders;
      }
      return Object.keys(out).length ? out : null;
    };
    const reply = (rs) => { try { blockingSocket && blockingSocket.send(JSON.stringify({ id: q.id, result: merge(rs) })); } catch (err) {} };
    if (results.some((r) => r && typeof r.then === 'function')) {
      Promise.allSettled(results).then((s) => reply(s.map((x) => x.value)));
    } else reply(results);
  }
  function connectBlocking() {
    if (blockingSocket || !blockingListeners) return;
    const ws = new WebSocket('ws://127.0.0.1:' + cfg.blockingPort + '/' + cfg.token);
    blockingSocket = ws;
    ws.onmessage = (m) => { let q; try { q = JSON.parse(m.data); } catch (err) { return; } answerBlocking(q); };
    ws.onclose = () => { if (blockingSocket === ws) blockingSocket = null; if (blockingListeners) setTimeout(connectBlocking, 1000); };
  }
  const webRequestHooks = (name) => ({
    add: (cb, extra) => {
      call('webRequest._listen', name, extra[0] || { urls: ['<all_urls>'] }, isBlocking(extra) ? extra[1] : (extra[1] || []).filter((x) => x !== 'blocking')).catch(() => {});
      if (isBlocking(extra)) { blockingListeners++; connectBlocking(); }
    },
    remove: (cb, n, extra) => {
      call('webRequest._unlisten', name, isBlocking(extra)).catch(() => {});
      if (isBlocking(extra) && --blockingListeners === 0 && blockingSocket) { const ws = blockingSocket; blockingSocket = null; ws.close(); }
    },
    // Blocking listeners only run through answerBlocking().
    filter: (args, extra) => !isBlocking(extra) && webRequestMatches(extra[0], args[0] || {}),
  });

  // ---- i18n -------------------------------------------------------------
  function subst(msg, ph, subs) {
    return String(msg).replace(/\$(\$|[1-9]|[A-Za-z0-9_@]+\$)/g, (all, g) => {
      if (g === '$') return '$';
      if (/^[1-9]$/.test(g)) return subs[+g - 1] !== undefined ? String(subs[+g - 1]) : '';
      const key = g.slice(0, -1).toLowerCase();
      const e = ph && Object.entries(ph).find(([k]) => k.toLowerCase() === key);
      return e ? subst(e[1].content || '', null, subs) : all;
    });
  }
  function getMessage(name, subs) {
    if (subs === undefined || subs === null) subs = [];
    if (!Array.isArray(subs)) subs = [subs];
    const k = String(name).toLowerCase();
    const special = { '@@extension_id': cfg.id, '@@ui_locale': cfg.locale, '@@bidi_dir': 'ltr', '@@bidi_reversed_dir': 'rtl', '@@bidi_start_edge': 'left', '@@bidi_end_edge': 'right' };
    if (k in special) return special[k];
    const m = cfg.messages[k];
    return m ? subst(m.message || '', m.placeholders, subs) : '';
  }

  // ---- helpers ------------------------------------------------------------
  const getURL = (p) => base + String(p || '').replace(/^\//, '');
  const stub = (ns, names) => Object.fromEntries(names.map((n) => [n, (...a) => call(ns + '.' + n, ...a)]));
  const events = (ns, names, hooks = {}) => Object.fromEntries(names.map((n) => [n, ev(ns + '.' + n, hooks[n])]));
  async function imageDataToUrl(d) {
    const one = d && d.width ? d : d && Object.values(d)[0];
    if (!one || typeof document === 'undefined') return null;
    const c = document.createElement('canvas'); c.width = one.width; c.height = one.height;
    c.getContext('2d').putImageData(one, 0, 0);
    return c.toDataURL('image/png');
  }
  const actionNS = (ns) => {
    const a = stub(ns, ['setTitle', 'getTitle', 'setPopup', 'getPopup', 'setBadgeText', 'getBadgeText', 'setBadgeBackgroundColor', 'getBadgeBackgroundColor', 'setBadgeTextColor', 'getBadgeTextColor', 'enable', 'disable', 'isEnabled', 'openPopup', 'getUserSettings', 'show', 'hide', 'isShown']);
    a.setIcon = async (d) => {
      d = Object.assign({}, d);
      if (d.imageData) { d._dataUrl = await imageDataToUrl(d.imageData); delete d.imageData; }
      return call(ns + '.setIcon', d);
    };
    a.onClicked = ev(ns + '.onClicked');
    return a;
  };
  const storageArea = (area) => Object.assign(stub('storage.' + area, ['get', 'set', 'remove', 'clear', 'getBytesInUse', 'getKeys', 'setAccessLevel']), { onChanged: ev('storage.' + area + '.onChanged') });

  // ---- namespaces -------------------------------------------------------
  const runtime = {
    id: cfg.id,
    getURL,
    getManifest: () => clone(cfg.manifest),
    sendMessage(...a) {
      let extId = '', msg, opts;
      if (a.length >= 3) [extId, msg, opts] = a;
      else if (a.length === 2 && typeof a[0] === 'string' && (a[0] === cfg.id || /@|^\{/.test(a[0])) && (typeof a[1] !== 'object' || a[1] === null || !('includeTlsChannelId' in a[1]))) [extId, msg] = a;
      else [msg, opts] = a;
      return call('runtime.sendMessage', extId || '', msg === undefined ? null : msg);
    },
    connect(a, b) {
      let extId = '', info = {};
      if (typeof a === 'string') { extId = a; info = b || {}; } else info = a || {};
      const p = new Port(newPortId(), info.name || '');
      call('runtime.connect', p._id, extId, info).catch(() => {});
      return p;
    },
    onMessage: ev('runtime.onMessage'), onConnect: ev('runtime.onConnect'),
    onMessageExternal: ev('runtime.onMessageExternal'), onConnectExternal: ev('runtime.onConnectExternal'),
    onInstalled: ev('runtime.onInstalled'), onStartup: ev('runtime.onStartup'), onSuspend: ev('runtime.onSuspend'),
    onSuspendCanceled: ev('runtime.onSuspendCanceled'), onUpdateAvailable: ev('runtime.onUpdateAvailable'), onRestartRequired: ev('runtime.onRestartRequired'),
    ...stub('runtime', ['getPlatformInfo', 'getBrowserInfo', 'openOptionsPage', 'setUninstallURL', 'requestUpdateCheck', 'getBackgroundPage', 'sendNativeMessage', 'getContexts']),
    reload() { call('runtime.reload').catch(() => {}); },
    connectNative() { throw new Error('Native messaging is not supported by Kite'); },
    get lastError() { return lastError; },
  };
  const i18n = { getMessage, getUILanguage: () => cfg.locale.replace('_', '-'), ...stub('i18n', ['getAcceptLanguages', 'detectLanguage']) };
  const extension = {
    getURL, get lastError() { return lastError; }, inIncognitoContext: false,
    getViews: () => [], getBackgroundPage: () => null,
    ...stub('extension', ['isAllowedIncognitoAccess', 'isAllowedFileSchemeAccess']),
    sendMessage: (...a) => runtime.sendMessage(...a), onMessage: runtime.onMessage, onRequest: runtime.onMessage,
  };
  const storage = { local: storageArea('local'), sync: storageArea('sync'), session: storageArea('session'), managed: storageArea('managed'), onChanged: ev('storage.onChanged') };
  const menus = {
    ...stub('menus', ['update', 'remove', 'removeAll', 'refresh', 'overrideContext']),
    create(props, cb) {
      props = Object.assign({}, props);
      if (props.onclick) { const id = props.id = props.id || ('kite-' + Math.random().toString(36).slice(2)); const fn = props.onclick; delete props.onclick; menus.onClicked.addListener((info, tab) => { if (info.menuItemId === id) fn(info, tab); }); }
      const p = call('menus.create', props);
      p.then(() => cb && cb(), (e) => { lastError = e; try { cb && cb(); } finally { lastError = null; } });
      return props.id;
    },
    getTargetElement: (id) => (typeof document !== 'undefined' ? (globalThis.__kiteLastContextTarget || null) : null),
    ACTION_MENU_TOP_LEVEL_LIMIT: 6,
    onClicked: ev('menus.onClicked'), onShown: ev('menus.onShown'), onHidden: ev('menus.onHidden'),
  };
  const all = {
    runtime, i18n, extension, storage,
    dom: { openOrClosedShadowRoot: (el) => el.openOrClosedShadowRoot || el.shadowRoot || null },
  };
  if (isContent) {
    all.menus = { getTargetElement: menus.getTargetElement };
  } else {
    Object.assign(all, {
      tabs: {
        ...stub('tabs', ['query', 'get', 'getCurrent', 'create', 'update', 'remove', 'reload', 'duplicate', 'move', 'goBack', 'goForward', 'executeScript', 'insertCSS', 'removeCSS', 'captureVisibleTab', 'captureTab', 'getZoom', 'setZoom', 'getZoomSettings', 'setZoomSettings', 'highlight', 'discard', 'detectLanguage', 'toggleReaderMode', 'print', 'saveAsPDF', 'warmup', 'hide', 'show']),
        sendMessage: (tabId, msg, opts) => call('tabs.sendMessage', tabId, msg === undefined ? null : msg, opts || {}),
        connect(tabId, info) { info = info || {}; const p = new Port(newPortId(), info.name || ''); call('tabs.connect', p._id, tabId, info).catch(() => {}); return p; },
        TAB_ID_NONE: -1,
        ...events('tabs', ['onCreated', 'onUpdated', 'onRemoved', 'onActivated', 'onMoved', 'onHighlighted', 'onAttached', 'onDetached', 'onReplaced', 'onZoomChange'], { onUpdated: { filter: tabsUpdatedFilter } }),
      },
      windows: { ...stub('windows', ['get', 'getCurrent', 'getLastFocused', 'getAll', 'create', 'update', 'remove']), WINDOW_ID_NONE: -1, WINDOW_ID_CURRENT: -2, ...events('windows', ['onCreated', 'onRemoved', 'onFocusChanged']) },
      browserAction: actionNS('browserAction'),
      action: actionNS('action'),
      pageAction: actionNS('pageAction'),
      sidebarAction: { ...actionNS('sidebarAction'), ...stub('sidebarAction', ['setPanel', 'getPanel', 'open', 'close', 'toggle', 'isOpen']) },
      menus, contextMenus: menus,
      alarms: { ...stub('alarms', ['create', 'get', 'getAll', 'clear', 'clearAll']), onAlarm: ev('alarms.onAlarm') },
      scripting: {
        ...stub('scripting', ['insertCSS', 'removeCSS', 'registerContentScripts', 'unregisterContentScripts', 'getRegisteredContentScripts', 'updateContentScripts']),
        executeScript: (d) => { d = Object.assign({}, d); if (typeof d.func === 'function') d.func = d.func.toString(); return call('scripting.executeScript', d); },
        ExecutionWorld: { ISOLATED: 'ISOLATED', MAIN: 'MAIN' },
      },
      contentScripts: {
        register: async (opts) => { const r = await call('contentScripts.register', opts); return { unregister: () => call('contentScripts.unregister', r.id) }; },
      },
      permissions: { ...stub('permissions', ['contains', 'getAll', 'request', 'remove']), ...events('permissions', ['onAdded', 'onRemoved']) },
      management: { ...stub('management', ['getSelf', 'getAll', 'get', 'uninstallSelf', 'getPermissionWarningsByManifest']), ...events('management', ['onInstalled', 'onUninstalled', 'onEnabled', 'onDisabled']) },
      commands: { ...stub('commands', ['getAll', 'update', 'reset']), onCommand: ev('commands.onCommand') },
      cookies: { ...stub('cookies', ['get', 'getAll', 'set', 'remove', 'getAllCookieStores']), onChanged: ev('cookies.onChanged') },
      webRequest: {
        ...stub('webRequest', ['handlerBehaviorChanged', 'getSecurityInfo']),
        filterResponseData() { throw new Error('webRequest.filterResponseData is not supported by Kite'); },
        ...Object.fromEntries(['onBeforeRequest', 'onBeforeSendHeaders', 'onSendHeaders', 'onHeadersReceived', 'onAuthRequired', 'onResponseStarted', 'onBeforeRedirect', 'onCompleted', 'onErrorOccurred'].map((n) => [n, ev('webRequest.' + n, webRequestHooks(n))])),
        MAX_HANDLER_BEHAVIOR_CHANGED_CALLS_PER_10_MINUTES: 20,
      },
      webNavigation: {
        ...stub('webNavigation', ['getFrame', 'getAllFrames']),
        ...events('webNavigation', ['onBeforeNavigate', 'onCommitted', 'onDOMContentLoaded', 'onCompleted', 'onErrorOccurred', 'onCreatedNavigationTarget', 'onReferenceFragmentUpdated', 'onTabReplaced', 'onHistoryStateUpdated'], Object.fromEntries(['onBeforeNavigate', 'onCommitted', 'onDOMContentLoaded', 'onCompleted', 'onErrorOccurred', 'onReferenceFragmentUpdated', 'onHistoryStateUpdated'].map((n) => [n, { filter: navFilter }]))),
      },
      declarativeNetRequest: { ...stub('declarativeNetRequest', ['updateDynamicRules', 'getDynamicRules', 'updateSessionRules', 'getSessionRules', 'updateEnabledRulesets', 'getEnabledRulesets', 'isRegexSupported', 'getAvailableStaticRuleCount', 'setExtensionActionOptions', 'getMatchedRules', 'testMatchOutcome']), MAX_NUMBER_OF_DYNAMIC_RULES: 30000, MAX_NUMBER_OF_SESSION_RULES: 5000, GUARANTEED_MINIMUM_STATIC_RULES: 30000, MAX_NUMBER_OF_REGEX_RULES: 1000, onRuleMatchedDebug: ev('declarativeNetRequest.onRuleMatchedDebug') },
      history: { ...stub('history', ['search', 'getVisits', 'addUrl', 'deleteUrl', 'deleteRange', 'deleteAll']), ...events('history', ['onVisited', 'onVisitRemoved', 'onTitleChanged']) },
      bookmarks: { ...stub('bookmarks', ['get', 'getChildren', 'getRecent', 'getSubTree', 'getTree', 'search', 'create', 'move', 'update', 'remove', 'removeTree']), ...events('bookmarks', ['onCreated', 'onRemoved', 'onChanged', 'onMoved', 'onChildrenReordered', 'onImportBegan', 'onImportEnded']) },
      downloads: {
        download: async (opts) => {
          opts = Object.assign({}, opts);
          if (/^blob:/.test(opts.url)) {
            // blob: URLs only exist in this document; hand the bytes over as a data: URL.
            const blob = await (await fetch(opts.url)).blob();
            opts.url = await new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(r.result); r.onerror = rej; r.readAsDataURL(blob); });
          }
          return call('downloads.download', opts);
        },
        ...stub('downloads', ['search', 'pause', 'resume', 'cancel', 'getFileIcon', 'open', 'show', 'showDefaultFolder', 'erase', 'removeFile']), ...events('downloads', ['onCreated', 'onErased', 'onChanged']) },
      notifications: { ...stub('notifications', ['create', 'update', 'clear', 'getAll']), ...events('notifications', ['onClosed', 'onClicked', 'onButtonClicked', 'onShown']) },
      topSites: stub('topSites', ['get']),
      sessions: { ...stub('sessions', ['getRecentlyClosed', 'restore', 'setTabValue', 'getTabValue', 'removeTabValue', 'setWindowValue', 'getWindowValue', 'removeWindowValue', 'forgetClosedTab', 'forgetClosedWindow']), MAX_SESSION_RESULTS: 25, onChanged: ev('sessions.onChanged') },
      search: stub('search', ['get', 'search', 'query']),
      idle: { ...stub('idle', ['queryState', 'setDetectionInterval']), onStateChanged: ev('idle.onStateChanged') },
      theme: { ...stub('theme', ['getCurrent', 'update', 'reset']), onUpdated: ev('theme.onUpdated') },
      omnibox: { ...stub('omnibox', ['setDefaultSuggestion']), ...events('omnibox', ['onInputStarted', 'onInputChanged', 'onInputEntered', 'onInputCancelled', 'onDeleteSuggestion']) },
      browsingData: stub('browsingData', ['remove', 'removeCache', 'removeCookies', 'removeDownloads', 'removeFormData', 'removeHistory', 'removeLocalStorage', 'removePasswords', 'removePluginData', 'settings']),
      clipboard: stub('clipboard', ['setImageData']),
      identity: { getRedirectURL: (p) => 'https://' + cfg.id.replace(/[^a-z0-9]/gi, '').toLowerCase().slice(0, 32) + '.extensions.allizom.org/' + (p || ''), launchWebAuthFlow: (d) => call('identity.launchWebAuthFlow', d) },
      privacy: { network: {}, services: {}, websites: {} },
      proxy: { settings: {}, onRequest: ev('proxy.onRequest'), onError: ev('proxy.onError') },
      browserSettings: {},
    });
    // BrowserSetting objects (read-only stubs).
    const setting = (n) => ({ get: () => Promise.resolve({ value: undefined, levelOfControl: 'not_controllable' }), set: () => Promise.resolve(false), clear: () => Promise.resolve(false), onChange: ev('setting.' + n) });
    for (const k of ['networkPredictionEnabled', 'peerConnectionEnabled', 'webRTCIPHandlingPolicy', 'httpsOnlyMode', 'globalPrivacyControl']) all.privacy.network[k] = setting(k);
    for (const k of ['passwordSavingEnabled']) all.privacy.services[k] = setting(k);
    for (const k of ['thirdPartyCookiesAllowed', 'hyperlinkAuditingEnabled', 'referrersEnabled', 'resistFingerprinting', 'firstPartyIsolate', 'protectedContentEnabled', 'trackingProtectionMode', 'cookieConfig']) all.privacy.websites[k] = setting(k);
    all.proxy.settings = setting('proxy');
    for (const k of ['allowPopupsForUserEvents', 'cacheEnabled', 'closeTabsByDoubleClick', 'contextMenuShowEvent', 'ftpProtocolEnabled', 'homepageOverride', 'imageAnimationBehavior', 'newTabPageOverride', 'newTabPosition', 'openBookmarksInNewTabs', 'openSearchResultsInNewTabs', 'openUrlbarResultsInNewTabs', 'overrideDocumentColors', 'useDocumentFonts', 'webNotificationsDisabled', 'zoomFullPage', 'zoomSiteSpecific']) all.browserSettings[k] = setting(k);
  }

  // ---- dispatch from the runtime -----------------------------------------
  function dispatch(p) {
    switch (p.type) {
      case 'event': {
        const e = registry.get(p.name);
        if (e) e._fire(p.args || []);
        break;
      }
      case 'message': return handleMessage(runtime.onMessage, p);
      case 'messageExternal': return handleMessage(runtime.onMessageExternal, p);
      case 'connect': {
        const port = new Port(p.portId, p.name, p.sender);
        if (!runtime.onConnect.hasListeners()) { port.disconnect(); return; }
        runtime.onConnect._l.slice().forEach(({ cb }) => { try { cb(port); } catch (e) { console.error(e); } });
        break;
      }
      case 'portMessage': { const port = ports.get(p.portId); if (port) port.onMessage._l.slice().forEach(({ cb }) => { try { cb(clone(p.message), port); } catch (e) { console.error(e); } }); break; }
      case 'portDisconnect': {
        const port = ports.get(p.portId); if (!port) return;
        port._open = false; ports.delete(p.portId);
        if (p.error) port.error = { message: p.error };
        lastError = p.error ? { message: p.error } : null;
        port.onDisconnect._l.slice().forEach(({ cb }) => { try { cb(port); } catch (e) { console.error(e); } });
        lastError = null;
        break;
      }
    }
  }

  // ---- chrome.* (callback style, also returns promises) ------------------
  function wrapChrome(obj, seen = new WeakMap()) {
    if (seen.has(obj)) return seen.get(obj);
    const out = {}; seen.set(obj, out);
    for (const key of Object.keys(obj)) {
      const desc = Object.getOwnPropertyDescriptor(obj, key);
      if (desc.get) { Object.defineProperty(out, key, { get: desc.get, enumerable: true }); continue; }
      const v = obj[key];
      if (typeof v === 'function' && !(v instanceof KiteEvent)) {
        out[key] = function (...args) {
          const cb = typeof args[args.length - 1] === 'function' ? args.pop() : null;
          let r;
          try { r = v.apply(obj, args); } catch (e) { if (!cb) throw e; lastError = { message: e.message }; try { cb(); } finally { lastError = null; } return; }
          if (!r || typeof r.then !== 'function' || key === 'create' && obj === menus) return r;
          if (!cb) return r;
          r.then((res) => { try { cb(res); } catch (e) { console.error(e); } }, (e) => { lastError = { message: e && e.message || String(e) }; try { cb(); } catch (x) { console.error(x); } finally { lastError = null; } });
        };
      } else if (v && typeof v === 'object' && !(v instanceof KiteEvent) && !Array.isArray(v)) out[key] = wrapChrome(v, seen);
      else out[key] = v;
    }
    return out;
  }
  const chrome = wrapChrome(all);
  // In Chrome, the MV2 `browserAction` and MV3 `action` are separate names.
  return { browser: all, chrome, dispatch };
}
