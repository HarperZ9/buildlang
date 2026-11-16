const { chromium } = require('E:/wt/site-all/node_modules/playwright');
const { contrastProbe, brightestGround, launch } = require('E:/wt/site-all/tests/lib/surfaces-probe.cjs');
(async () => {
  const b = await launch();
  const p = await (await b.newContext({ viewport: { width: 1280, height: 800 }, reducedMotion: 'reduce' })).newPage();
  await p.goto(process.argv[2], { waitUntil: 'load' });
  await p.waitForSelector('html[data-surfaces-ready], #parts[data-surface-live="bench"]', { state: 'attached', timeout: 45000 });
  const br = await brightestGround(p);
  const c = await p.evaluate(contrastProbe, br);
  const all = await p.evaluate((ground) => {
    const lin = (c) => { c /= 255; return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
    const L = ([r, g, b]) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    const rows = {};
    for (const el of document.querySelectorAll('.parts-lead, .bn *, .parts-surface *')) {
      if (![...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim())) continue;
      const cs = getComputedStyle(el); if (cs.visibility === 'hidden' || cs.display === 'none' || el.closest('[hidden], .sr-only, canvas, svg, .bl-dot, .bl-n, .lt-word, .tm-tip')) continue;
      const m = cs.color.match(/[\d.]+/g).map(Number); if ((m[3] ?? 1) < 1) continue;
      const r = ((L(m) + 0.05) / (ground + 0.05)).toFixed(2);
      const k = cs.color + ' ' + r; rows[k] = (rows[k] || 0) + 1;
    }
    return rows;
  }, Math.max(c.ceiling, br));
  console.log(JSON.stringify({ brightest: br, ceiling: c.ceiling, blend: c.blend, worst: c.worst, all }, null, 1));
  await b.close();
})();
