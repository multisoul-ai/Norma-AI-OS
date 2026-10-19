#!/usr/bin/env node
// Capture the Aurora WebGL shader from docs/prototypes/norma-assistant/index.html.
// The shader adapts React Bits Orb by David Haz; see THIRD_PARTY_NOTICES.md.
// Usage: node scripts/generate-norma-orb-frames.cjs [Chrome executable path]

const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { chromium } = require('playwright');

const root = path.resolve(__dirname, '..');
const prototype = path.join(root, 'docs/prototypes/norma-assistant/index.html');
const output = path.join(root, 'crates/app/assets/icons');
const frameCount = 32;
const orbitDuration = (2 * Math.PI) / 0.28;

async function main() {
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.argv[2] || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    args: ['--enable-webgl', '--use-gl=angle', '--use-angle=swiftshader'],
  });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
    const url = pathToFileURL(prototype).href;
    await page.route(url, async (route) => {
      const source = fs.readFileSync(prototype, 'utf8');
      const end = source.lastIndexOf('})();');
      if (end < 0) throw new Error('Could not locate the prototype orb closure');
      const capture = `
window.__captureNormaOrb = (time, energy) => {
  if (!gl || !program) throw new Error('Prototype WebGL orb is unavailable');
  canvas.width = 288;
  canvas.height = 288;
  gl.viewport(0, 0, 288, 288);
  state.mode = energy > .1 ? 'listening' : 'idle';
  presence.time = time;
  presence.energy = energy;
  renderOrb(performance.now(), 0);
  // Preserve the prototype's entire 144px canvas, including the transparent
  // optical margin. Cropping it shifts the ring relative to the copy.
  return canvas.toDataURL('image/png');
};
`;
      await route.fulfill({ status: 200, contentType: 'text/html', body: source.slice(0, end) + capture + source.slice(end) });
    });
    await page.goto(url);
    await page.waitForFunction(() => typeof window.__captureNormaOrb === 'function');
    for (const variant of [{ suffix: '', energy: .1 }, { suffix: '-active', energy: .8 }]) {
      for (let index = 0; index < frameCount; index += 1) {
        const time = 7 + (index / frameCount) * orbitDuration;
        const png = await page.evaluate(({ time, energy }) => window.__captureNormaOrb(time, energy), { time, energy: variant.energy });
        fs.writeFileSync(path.join(output, `norma-orb${variant.suffix}-${String(index).padStart(2, '0')}.png`), Buffer.from(png.split(',')[1], 'base64'));
      }
    }
  } finally {
    await browser.close();
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
