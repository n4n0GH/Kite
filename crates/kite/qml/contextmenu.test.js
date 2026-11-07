// Run with: node --test crates/kite/qml/contextmenu.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/contextmenu.js', 'utf8').replace('.pragma library', '');
const { linkLeadsToImage } = new Function(src + '\nreturn {linkLeadsToImage};')();

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
