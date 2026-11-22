// Run with: node --test crates/kite/qml/mediafile.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const src = fs.readFileSync(__dirname + '/mediafile.js', 'utf8').replace('.pragma library', '');
const { fileName, savable, isPage, documentName } = new Function(src + '\nreturn {fileName, savable, isPage, documentName};')();

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

test('a page is saved as a page, anything else a tab shows as the file it is', () => {
  assert.ok(isPage('text/html'));
  assert.ok(isPage('application/xhtml+xml'));
  assert.ok(isPage('TEXT/HTML'));
  assert.ok(isPage(''), 'a tab that does not say is taken for a page');
  assert.ok(isPage(undefined));
  for (const t of ['application/pdf', 'image/png', 'image/svg+xml', 'video/mp4', 'audio/mpeg', 'text/plain', 'text/xml', 'application/json', 'text/css'])
    assert.ok(!isPage(t), t);
});

test('a shown file the engine downloads keeps the name the engine gives it', () => {
  assert.strictEqual(documentName('https://a.example/get?id=5', 'application/pdf', 'report 2026.pdf', false), 'report 2026.pdf');
  assert.strictEqual(documentName('https://a.example/doc.pdf', 'application/pdf', 'doc.pdf', false), 'doc.pdf');
  assert.strictEqual(documentName('https://a.example/photo', 'image/jpeg', 'photo.jpg', false), 'photo.jpg');
  assert.strictEqual(documentName('https://a.example/notes.txt', 'application/pdf', '', false), 'notes.txt', 'without one, the address names it');
});

test('shown text is named by its address, not as a saved page', () => {
  assert.strictEqual(documentName('https://a.example/notes.txt', 'text/plain', 'notes.mhtml', true), 'notes.txt');
  assert.strictEqual(documentName('https://a.example/src/main.rs?raw=1', 'text/plain', 'main.mhtml', true), 'main.rs');
  assert.strictEqual(documentName('https://a.example/feed.rss', 'text/xml', 'feed.mhtml', true), 'feed.rss');
  assert.strictEqual(documentName('https://a.example/raw', 'text/plain', 'raw.mhtml', true), 'raw.txt');
  assert.strictEqual(documentName('https://a.example/feed', 'text/xml', 'feed.mhtml', true), 'feed.xml');
  assert.strictEqual(documentName('https://a.example/style', 'text/css', 'style.mhtml', true), 'style.css');
  assert.strictEqual(documentName('https://a.example/app', 'text/javascript', 'app.mhtml', true), 'app.js');
  assert.strictEqual(documentName('https://a.example/app', 'application/x-javascript', 'app.mhtml', true), 'app.js');
  assert.strictEqual(documentName('https://a.example/', 'text/plain', 'a.example.mhtml', true), 'a.example.txt', 'no name in the address: the engine\'s, without its ending');
  assert.strictEqual(documentName('https://a.example/', 'text/plain', '', true), 'file.txt');
  assert.strictEqual(documentName('https://a.example/a/..%2F..%2Fetc%2Fpasswd', 'text/plain', 'x.mhtml', true), '_.._etc_passwd', 'and it can not leave the folder');
});
