const { chromium } = require('E:/wt/site-all/node_modules/playwright');
(async () => {
  const b = await chromium.launch({ channel: 'chrome', args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const p = await (await b.newContext({ viewport: { width: 1280, height: 800 }, reducedMotion: 'reduce' })).newPage();
  await p.goto(process.argv[2], { waitUntil: 'load' });
  await p.waitForSelector('html[data-surfaces-ready]', { state: 'attached', timeout: 45000 });
  const r = await p.evaluate(() => {
    const out = [];
    for (const el of document.querySelectorAll('.parts-surface *')) {
      if (![...el.childNodes].some((n) => n.nodeType === 3 && /overshot|twill|plain|satin/.test(n.textContent))) continue;
      const cs = getComputedStyle(el), par = el.parentElement, pcs = getComputedStyle(par);
      out.push({ tag: el.tagName, cls: el.className, text: el.textContent.trim().slice(0, 30), color: cs.color, bg: cs.backgroundColor, parent: par.tagName + '.' + par.className, pcolor: pcs.color, pbg: pcs.backgroundColor, sheet: cs.colorScheme });
    }
    const lid = getComputedStyle(document.querySelector('.gw-backdrop'), '::before');
    return { out: out.slice(0, 8), ceiling: lid.backgroundColor, sel: [...document.querySelectorAll('.parts-surface select')].map((s) => ({ cls: s.className, data: Object.keys(s.dataset), color: getComputedStyle(s).color, bg: getComputedStyle(s).backgroundColor })).slice(0, 6) };
  });
  console.log(JSON.stringify(r, null, 1));
  await b.close();
})();
