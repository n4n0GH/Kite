'use strict';
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const src = fs.readFileSync(path.join(__dirname, 'ytdl.js'), 'utf8').replace('.pragma library', '');
const Y = new Function(src + '\nreturn {videoUrl, titleOf, QUALITIES, size};')();
const plugin = JSON.parse(fs.readFileSync(path.join(__dirname, 'plugin.json'), 'utf8'));

test('video pages, and only those', () => {
  const v = 'https://www.youtube.com/watch?v=dQw4w9WgXcQ';
  assert.strictEqual(Y.videoUrl('https://www.youtube.com/watch?v=dQw4w9WgXcQ'), v);
  assert.strictEqual(Y.videoUrl('https://www.youtube.com/watch?list=PL123&v=dQw4w9WgXcQ&index=2'), v);
  assert.strictEqual(Y.videoUrl('https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=42s'), v);
  assert.strictEqual(Y.videoUrl('https://music.youtube.com/watch?v=dQw4w9WgXcQ'), v);
  assert.strictEqual(Y.videoUrl('https://youtube.com/shorts/dQw4w9WgXcQ'), v);
  assert.strictEqual(Y.videoUrl('https://www.youtube.com/live/dQw4w9WgXcQ?si=x'), v);
  assert.strictEqual(Y.videoUrl('https://youtu.be/dQw4w9WgXcQ?t=1'), v);
  for (const no of ['https://www.youtube.com/', 'https://www.youtube.com/@someone', 'https://www.youtube.com/results?search_query=v%3D',
    'https://www.youtube.com/watch?v=short', 'https://www.youtube.com/watch?v=dQw4w9WgXcQx', 'https://notyoutube.com/watch?v=dQw4w9WgXcQ',
    'https://www.youtube.com.evil.example/watch?v=dQw4w9WgXcQ', 'https://evil.example/?u=https://youtu.be/dQw4w9WgXcQ', 'kite:start', '']) {
    assert.strictEqual(Y.videoUrl(no), '', no);
  }
});

test('the video name from the tab title', () => {
  assert.strictEqual(Y.titleOf('(3) Some Video - YouTube'), 'Some Video');
  assert.strictEqual(Y.titleOf('Some - Video - YouTube'), 'Some - Video');
  assert.strictEqual(Y.titleOf(undefined), '');
});

test('sizes, and the qualities in the manifest', () => {
  assert.strictEqual(Y.size(512), '512 B');
  assert.strictEqual(Y.size(12345678), '12.3 MB');
  assert.ok(Y.QUALITIES.some((q) => q.k === plugin.settings.quality));
});
