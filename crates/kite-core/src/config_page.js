// The page of kite:config. It runs no code from settings: values only ever go into
// .textContent and .value. Changes go to the browser as console messages with the
// page's token (WebTab.qml), and the answers come back through __kiteConfigDone.
(function () {
  'use strict';
  const data = JSON.parse(document.getElementById('data').textContent);
  const rows = document.getElementById('rows');
  const search = document.getElementById('search');
  const onlyChanged = document.getElementById('changed');
  const count = document.getElementById('count');
  const entries = new Map(data.entries.map((e) => [e.key, e]));

  const send = (op, key, value) =>
    console.debug('kite-config:' + JSON.stringify({ token: data.token, op, key, value }));

  function text(entry) {
    if (entry.kind === 'string') return entry.value === null ? '' : entry.value;
    return JSON.stringify(entry.value, null, entry.kind === 'list' || entry.kind === 'object' ? 2 : 0);
  }

  function element(tag, className, content) {
    const e = document.createElement(tag);
    if (className) e.className = className;
    if (content !== undefined) e.textContent = content;
    return e;
  }

  function editor(entry, error) {
    if (entry.kind === 'boolean') {
      const b = element('button', 'toggle', entry.value ? 'true' : 'false');
      b.title = 'Switch';
      b.onclick = () => send('set', entry.key, entry.value ? 'false' : 'true');
      return b;
    }
    const long = entry.kind === 'list' || entry.kind === 'object';
    const input = element(long ? 'textarea' : 'input', 'value');
    if (long) input.rows = Math.min(10, text(entry).split('\n').length);
    else input.type = 'text';
    input.value = text(entry);
    input.spellcheck = false;
    input.setAttribute('aria-label', entry.key);
    const commit = () => { if (input.value !== text(entry)) send('set', entry.key, input.value); };
    input.onblur = commit;
    input.onkeydown = (e) => {
      if (e.key === 'Enter' && (!long || e.ctrlKey)) { e.preventDefault(); commit(); }
      if (e.key === 'Escape') { input.value = text(entry); input.blur(); }
    };
    if (error) input.classList.add('bad');
    return input;
  }

  function row(entry, error) {
    const tr = element('tr', entry.modified ? 'modified' : '');
    tr.dataset.key = entry.key;
    tr.appendChild(element('td', 'key', entry.key));
    const value = element('td');
    value.appendChild(editor(entry, error));
    if (error) value.appendChild(element('div', 'error', error));
    tr.appendChild(value);
    tr.appendChild(element('td', 'kind', entry.kind));
    const reset = element('td');
    if (entry.modified) {
      const b = element('button', '', 'Reset');
      b.title = entry.hasDefault ? 'Back to its default' : 'Forget this value';
      b.onclick = () => send('reset', entry.key);
      reset.appendChild(b);
    }
    tr.appendChild(reset);
    return tr;
  }

  function shown(entry) {
    const words = search.value.trim().toLowerCase().split(/\s+/).filter((w) => w);
    if (onlyChanged.checked && !entry.modified) return false;
    const haystack = entry.key.toLowerCase();
    return words.every((w) => haystack.includes(w));
  }

  function render() {
    rows.textContent = '';
    let n = 0;
    for (const entry of data.entries) {
      if (!shown(entry)) continue;
      rows.appendChild(row(entry));
      n += 1;
    }
    count.textContent = n + ' of ' + data.entries.length;
  }

  // The browser's answer to a change.
  window.__kiteConfigDone = (answer) => {
    const old = rows.querySelector('tr[data-key="' + CSS.escape(answer.key) + '"]');
    const entry = answer.entry || entries.get(answer.key);
    if (answer.entry) {
      entries.set(answer.key, entry);
      data.entries[data.entries.findIndex((e) => e.key === answer.key)] = entry;
    }
    if (!old) return;
    // The new row takes over from the old one: its keyboard focus, and what was typed
    // if that was refused. (Removing a focused field would commit it again.)
    const field = old.querySelector('.value, .toggle');
    const focused = field === document.activeElement;
    const typed = field && field.classList.contains('value') ? field.value : null;
    if (field) field.onblur = null;
    const fresh = row(entry, answer.error);
    old.replaceWith(fresh);
    const now = fresh.querySelector('.value, .toggle');
    if (answer.error && typed !== null) now.value = typed;
    if (focused) now.focus();
  };

  search.oninput = render;
  onlyChanged.onchange = render;
  document.getElementById('accept').onclick = () => {
    document.getElementById('warning').hidden = true;
    document.getElementById('main').hidden = false;
    render();
    search.focus();
  };
  document.getElementById('accept').focus();
})();
