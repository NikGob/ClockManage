// Packs src-tauri/icons/icon.ico from icon.svg (64 px and up) and icon-small.svg (16–48 px).
// Needs Playwright with Chromium: node scripts/make-ico.cjs
// Render the two SVGs to PNGs and pack icon.ico (PNG entries, Vista+), small sizes from the bold variant.
const { chromium } = require('playwright');
const fs = require('fs');
const dir = require('path').join(__dirname, '../src-tauri/icons/');
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage();
  const render = async (svg, n) => {
    await p.setViewportSize({ width: n, height: n });
    await p.setContent(`<html><body style="margin:0;background:transparent"><img src="data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}" width="${n}" height="${n}" style="display:block"></body></html>`);
    await p.waitForTimeout(50);
    return p.screenshot({ omitBackground: true, clip: { x: 0, y: 0, width: n, height: n } });
  };
  const big = fs.readFileSync(dir + 'icon.svg', 'utf8');
  const small = fs.readFileSync(dir + 'icon-small.svg', 'utf8');
  const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
  const imgs = [];
  // 16–20 px: no ring at all, just the lock and its keyhole.
  const tiny = small.replace(/<path d="M[^"]*A[^"]*" fill="none"[^>]*\/>/, '');
  for (const n of sizes) imgs.push({ n, png: await render(n <= 20 ? tiny : n <= 48 ? small : big, n) });
  // 32x32.png is what the window/taskbar falls back to: the bold one there too.
  fs.writeFileSync(dir + '32x32.png', imgs.find((i) => i.n === 32).png);
  const head = Buffer.alloc(6 + 16 * imgs.length);
  head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(imgs.length, 4);
  let off = head.length;
  imgs.forEach((im, i) => {
    const e = 6 + i * 16;
    head.writeUInt8(im.n >= 256 ? 0 : im.n, e); head.writeUInt8(im.n >= 256 ? 0 : im.n, e + 1);
    head.writeUInt8(0, e + 2); head.writeUInt8(0, e + 3);
    head.writeUInt16LE(1, e + 4); head.writeUInt16LE(32, e + 6);
    head.writeUInt32LE(im.png.length, e + 8); head.writeUInt32LE(off, e + 12);
    off += im.png.length;
  });
  fs.writeFileSync(dir + 'icon.ico', Buffer.concat([head, ...imgs.map((i) => i.png)]));
  console.log('ico', fs.statSync(dir + 'icon.ico').size, 'bytes,', sizes.join(','));
  await b.close();
})();
