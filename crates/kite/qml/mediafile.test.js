// Run with: node --test crates/kite/qml/mediafile.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/mediafile.js', 'utf8').replace('.pragma library', '');
const { fileName, savable } = new Function(src + '\nreturn {fileName, savable};')();

test('the last part of the address is the name', () => {
  assert.strictEqual(fileName('https://a.example/img/photo.jpg', 'image'), 'photo.jpg');
  assert.strictEqual(fileName('https://a.example/img/photo.jpg?w=300&h=200#top', 'image'), 'photo.jpg');
  assert.strictEqual(fileName('https://a.example/My%20Holiday%20%231.png', 'image'), 'My Holiday #1.png');
  assert.strictEqual(fileName('https://a.example/clips/', 'video'), 'clips');
});

test('an address without a name gets its kind', () => {
  assert.strictEqual(fileName('https://a.example/', 'image'), 'image');
  assert.strictEqual(fileName('https://a.example', 'background'), 'background');
  assert.strictEqual(fileName('https://a.example/?id=7', ''), 'file');
  assert.strictEqual(fileName('data:image/png;base64,iVBORw0KGgo=', 'image'), 'image.png');
  assert.strictEqual(fileName('data:image/svg+xml,%3Csvg/%3E', 'icon'), 'icon.svg');
  assert.strictEqual(fileName('data:,x', 'image'), 'image');
});

test('a name can not leave the folder, hide, or hold what a file system minds', () => {
  assert.strictEqual(fileName('https://a.example/a/..%2F..%2Fetc%2Fpasswd', 'image'), '_.._etc_passwd');
  assert.strictEqual(fileName('https://a.example/x/%2E%2E', 'image'), 'image');
  assert.strictEqual(fileName('https://a.example/.bashrc', 'image'), 'bashrc');
  assert.strictEqual(fileName('https://a.example/a%00b%0Ac.png', 'image'), 'a_b_c.png');
  assert.strictEqual(fileName('https://a.example/what%3F%3A%22%3C%3E%7C*.png', 'image'), 'what_______.png');
  assert.strictEqual(fileName('https://a.example/%E0%A4%A.png', 'image'), '%E0%A4%A.png', 'what is no encoding stays as written');
  for (const url of ['https://a.example/a%2Fb.png', 'https://a.example/a%5Cb.png']) assert.ok(!/[\/\\]/.test(fileName(url, 'image')), url);
});

test('a long name is cut, and keeps what kind of file it is', () => {
  const name = fileName('https://a.example/' + 'x'.repeat(300) + '.jpeg', 'image');
  assert.strictEqual(name.length, 120);
  assert.ok(name.endsWith('.jpeg'));
});

test('only what any view can fetch is saved', () => {
  assert.ok(savable('https://a.example/a.png'));
  assert.ok(savable('http://a.example/a.png'));
  assert.ok(savable('data:image/png;base64,AAAA'));
  assert.ok(!savable('blob:https://a.example/3f2a'));
  assert.ok(!savable('file:///etc/passwd'));
  assert.ok(!savable('javascript:alert(1)'));
});
