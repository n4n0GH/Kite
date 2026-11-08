// The page of kite:welcome. Choices go to the browser as console messages with the
// page's token (WebTab.qml), and the answer comes back through __kiteWelcomeDone.
// Browser names only ever go into .textContent.
(function () {
  'use strict';
  const data = JSON.parse(document.getElementById('data').textContent);
  const $ = (id) => document.getElementById(id);
  const send = (message) => console.debug('kite-welcome:' + JSON.stringify(Object.assign({ token: data.token }, message)));

  let source = null; // what to import from, if anything
  let level = 'high';

  function show(step) {
    for (const id of ['import', 'privacy', 'ready']) $(id).hidden = id !== step;
    document.querySelectorAll('.steps li').forEach((li, i) => li.classList.toggle('on', i === ['import', 'privacy', 'ready'].indexOf(step)));
    const first = $(step).querySelector('input, button.primary:not(:disabled), button');
    if (first) first.focus();
  }

  // ---- step 1: import ----
  if (data.sources.length === 0) {
    $('none').hidden = false;
    $('do-import').hidden = true;
    $('skip-import').textContent = 'Next';
    $('skip-import').classList.add('primary');
  } else {
    $('where').hidden = false;
    data.sources.forEach((s, i) => {
      const label = document.createElement('label');
      const input = document.createElement('input');
      input.type = 'radio';
      input.name = 'source';
      input.value = String(i);
      input.onchange = () => {
        source = data.sources[i].source;
        $('do-import').disabled = false;
      };
      label.append(input, document.createTextNode(s.label));
      $('sources').append(label);
    });
  }
  $('skip-import').onclick = () => { source = null; show('privacy'); };
  $('do-import').onclick = () => { if (source) show('privacy'); };

  // ---- step 2: privacy level ----
  const levels = [
    {
      key: 'low', name: 'Low',
      text: 'Everything is let through, for browsing without interruptions.',
      points: ['Ads and trackers aren\'t blocked', 'All cookies are allowed, from any site', 'Pages may load over unencrypted connections', 'Links keep their tracking parameters, and sites aren\'t asked not to track you', 'Private windows show your history and notifications'],
      ack: 'I am aware that this setting is considered insecure and may endanger my system',
      warning: 'Low turns off Kite\'s privacy and security protections. Advertisers can follow you from site to site, and pages can load over connections that others on your network can read or change. Choose it only if you accept that risk.',
    },
    {
      key: 'medium', name: 'Medium',
      text: 'Keeps your browsing private from people looking at your screen, and blocks the worst of the web.',
      points: ['Ads and trackers are blocked', 'Pages load over secure connections (HTTPS) when sites offer them', 'Private windows show no history, suggestions or notifications', 'All cookies are allowed, and sites aren\'t asked not to track you'],
    },
    {
      key: 'high', name: 'High (recommended)',
      text: 'The most protection that still keeps everyday sites working as they should.',
      points: ['Ads, trackers and third-party cookies are blocked', 'Tracking parameters are removed from links, and link click pings are blocked', 'Sites are asked not to track you or sell your data', 'Pages load over secure connections (HTTPS) when sites offer them', 'Your local network address is hidden from video calls', 'Private windows show no history, suggestions or notifications'],
    },
    {
      key: 'extreme', name: 'Extreme',
      text: 'The safest, at the cost of many sites not working until you let them.',
      points: ['Nothing is remembered: no history, and cookies and cache are cleared when Kite closes', 'All cookies are blocked, unless you allow them for a site', 'Sites aren\'t told which page linked to them', 'Fingerprinting protection can be turned on for sites', 'Container tabs keep sites apart', 'Extensions are turned off', 'Everything from High as well'],
      ack: 'I am aware that this setting will break most websites',
      warning: 'Sites you sign in to, shops and many others will not work until you allow their cookies, from the site menu in the address bar.',
    },
  ];
  function showLevel() {
    const l = levels[Number($('level').value)];
    level = l.key;
    $('level-name').textContent = l.name;
    $('level-text').textContent = l.text;
    $('level-points').replaceChildren(...l.points.map((p) => { const li = document.createElement('li'); li.textContent = p; return li; }));
    $('level-warning').hidden = !l.warning;
    $('level-warning').textContent = l.warning || '';
    $('use-level').textContent = 'Use ' + l.name.replace(/ \(.*\)$/, '') + ' Level';
    // Low and Extreme only once their cost is acknowledged, anew for each choice.
    $('ack').hidden = !l.ack;
    $('ack-text').textContent = l.ack || '';
    $('ack-box').checked = false;
    $('use-level').disabled = !!l.ack;
    $('level').setAttribute('aria-valuetext', l.name);
  }
  $('level').oninput = showLevel;
  $('ack-box').onchange = () => { $('use-level').disabled = !$('ack-box').checked; };
  showLevel();
  $('skip-level').onclick = () => { level = 'high'; finish(); };
  // Back to the import step, as it was left (the browser chosen stays chosen).
  $('back').onclick = () => show('import');
  $('use-level').onclick = () => { if (!$('use-level').disabled) finish(); };

  // ---- step 3: setting up ----
  let answered = false;
  let shownLongEnough = false;
  function finish() {
    show('ready');
    send({ op: 'finish', level, source });
    // Long enough to be seen, even when there's nothing to import.
    setTimeout(() => { shownLongEnough = true; maybeDone(); }, 1600);
  }
  function maybeDone() {
    if (!answered || !shownLongEnough) return;
    $('working').classList.add('leaving');
    setTimeout(() => {
      $('working').hidden = true;
      $('done').hidden = false;
      // The fade in, then a moment, then the start page.
      setTimeout(() => send({ op: 'go' }), 500 + 500);
    }, 400);
  }
  window.__kiteWelcomeDone = () => { answered = true; maybeDone(); };
})();
