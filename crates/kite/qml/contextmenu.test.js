// Run with: node --test crates/kite/qml/contextmenu.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/contextmenu.js', 'utf8').replace('.pragma library', '');
const { linkLeadsToImage, linkOpenable } = new Function(src + '\nreturn {linkLeadsToImage, linkOpenable};')();

test('a thumbnail linking to its full picture', () => {
  assert.ok(linkLeadsToImage('https://i.4cdn.org/g/123.jpg', 'https://i.4cdn.org/g/123s.jpg', true));
  assert.ok(linkLeadsToImage('https://x.example/a.PNG?size=big#top', 'https://x.example/t.png', true));
  assert.ok(linkLeadsToImage('https://x.example/a.jpg:orig', 'https://x.example/t.png', true));
});

test('the same address twice, whatever it is', () => {
  assert.ok(linkLeadsToImage('https://x.example/picture', 'https://x.example/picture', true));
});

test('an image in a link to something else keeps both', () => {
  assert.ok(!linkLeadsToImage('https://x.example/article.html', 'https://x.example/t.png', true));
  assert.ok(!linkLeadsToImage('https://x.example/page?file=a.png', 'https://x.example/t.png', true));
  assert.ok(!linkLeadsToImage('https://x.example/png', 'https://x.example/t.png', true));
});

test('nothing to merge', () => {
  assert.ok(!linkLeadsToImage('', 'https://x.example/t.png', true), 'no link');
  assert.ok(!linkLeadsToImage('https://x.example/a.png', '', false), 'a link to an image, on text');
});

test('a website\'s links open to the web or other apps only', () => {
  const page = 'https://evil.example/';
  for (const ok of ['https://kde.org/', 'http://a.b/', 'mailto:a@b.c', 'magnet:?xt=1', 'ftp://a/'])
    assert.ok(linkOpenable(ok, page), ok);
  for (const bad of ['kite:start?add=https://x', 'KITE:config', 'file:///etc/passwd', 'data:text/html,x', 'javascript:alert(1)', 'view-source:https://a', 'moz-extension://u/p.html', 'about:blank', ''])
    assert.ok(!linkOpenable(bad, page), bad);
});

test('pages of one kind link to their own kind', () => {
  assert.ok(linkOpenable('kite:config', 'kite:start'));
  assert.ok(linkOpenable('file:///home/a/b.html', 'file:///home/a/index.html'));
  assert.ok(linkOpenable('moz-extension://u/b.html', 'moz-extension://u/a.html'));
  assert.ok(!linkOpenable('file:///etc/passwd', 'kite:start'));
});

