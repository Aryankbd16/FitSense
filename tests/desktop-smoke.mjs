import assert from 'node:assert/strict';
import { openWebview, until } from './webview-cdp.mjs';

const [port, screenshots] = process.argv.slice(2);
const webview = await openWebview(port, screenshots);
const { send, evaluate, click, screenshot, requests, browserErrors } = webview;

try {
  const workspace = await until(() => evaluate('document.body?.innerText'),
    (body) => body?.includes('Connected') && body.includes('Schema 1 · portable mode'), 'Portable workspace did not connect')
    .catch(async (error) => {
      console.error('Requests:', [...requests]);
      console.error('WebView errors:', browserErrors.slice(0, 10));
      console.error('Document:', await evaluate('document.documentElement?.outerHTML.slice(0, 1400)'));
      throw error;
    });
  assert.match(workspace, /Private storage is locked/);
  assert.match(workspace, /Active\s+Local logs/);
  await until(() => evaluate(`Array.from(document.fonts).some((font) => font.family.includes('Manrope Variable') && font.status === 'loaded')`),
    Boolean, 'The bundled Manrope font did not load');
  await screenshot('workspace');

  await click('Open diagnostics');
  await click('Run diagnostics');
  await until(() => evaluate('document.querySelector(".diagnostic-results")?.innerText'),
    (text) => /Python\s+3\.13\.17\s+IPC protocol\s+v1\s+Response time\s+(?:<1|\d+) ms/.test(text), 'Diagnostic IPC result missing');
  await screenshot('diagnostics');

  const viewport = '({ viewport: window.innerWidth, client: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth })';
  await send('Emulation.setDeviceMetricsOverride', { width: 700, height: 700, deviceScaleFactor: 1, mobile: false });
  const checks = await until(() => evaluate(viewport), (size) => size?.viewport === 700, 'WebView did not resize to 700px');
  assert.ok(checks.scroll <= checks.client, `Diagnostics overflow at 700px: ${JSON.stringify(checks)}`);
  await click('Overview');
  const overview = await evaluate(viewport);
  assert.ok(overview.scroll <= overview.client, `Workspace overflow at 700px: ${JSON.stringify(overview)}`);
  await screenshot('workspace-700px');
  webview.assertLocalRequests();
  console.log('WebView2 rendered the local workspace, returned real worker diagnostics and fit a 700px viewport. Observed requests stayed within Tauri origins.');
} finally {
  webview.close();
}
