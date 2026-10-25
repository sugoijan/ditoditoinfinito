// Run with NODE_PATH pointing to a Playwright installation. The SVG and PNG
// are generated assets; fonts are not installed or needed at runtime.
const {chromium} = require('playwright');
const fs = require('node:fs');
const path = require('node:path');
(async () => {
  const browser = await chromium.launch({headless: true, channel: 'chrome'});
  try {
    const page = await browser.newPage();
    const svg = fs.readFileSync(path.join(__dirname, 'atlas.svg'), 'utf8');
    const png = await page.evaluate(async svg => {
      const img = new Image();
      img.src = 'data:image/svg+xml;base64,' + btoa(svg);
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width; c.height = img.height;
      c.getContext('2d').drawImage(img, 0, 0);
      return c.toDataURL('image/png').split(',')[1];
    }, svg);
    const output = path.resolve(__dirname, '../../assets/title');
    fs.mkdirSync(output, {recursive: true});
    fs.writeFileSync(path.join(output, 'dosis.png'), Buffer.from(png, 'base64'));
  } finally { await browser.close(); }
})();
