const { chromium } = require('E:/wt/site-all/node_modules/playwright');
(async () => {
  const b = await chromium.launch({ channel: 'chrome', args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
  const p = await (await b.newContext({ viewport: { width: 1280, height: 720 } })).newPage();
  const logs = [];
  p.on('pageerror', (e) => logs.push('PAGEERROR ' + e.message.slice(0, 300)));
  p.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') logs.push(m.type() + ' ' + m.text().slice(0, 300)); });
  p.on('requestfailed', (r) => logs.push('REQFAIL ' + r.url().slice(0, 160) + ' ' + (r.failure() || {}).errorText));
  p.on('response', (r) => { if (r.status() >= 400) logs.push('HTTP ' + r.status() + ' ' + r.url().slice(0, 160)); });
  await p.goto(process.argv[2], { waitUntil: 'load' });
  for (const t of [3, 10, 25]) {
    await p.waitForTimeout((t === 3 ? 3 : t === 10 ? 7 : 15) * 1000);
    console.log(t + 's', JSON.stringify(await p.evaluate(() => { const e = document.querySelector('#parts'); return { live: e && e.getAttribute('data-surface-live'), cells: document.querySelectorAll('.bn-cell').length }; })));
  }
  console.log(logs.slice(0, 25).join('\n'));
  await b.close();
})();
