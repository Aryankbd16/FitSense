import assert from 'node:assert/strict';
import { openWebview, until } from './webview-cdp.mjs';

const [port, screenshots, workflow, mode, fixture] = process.argv.slice(2);
const webview = await openWebview(port, screenshots);
const { click, evaluate, send } = webview;
const passphrase = 'FitSense smoke test recovery phrase';
const garmentName = 'Offline indigo shirt';
const changedName = 'Offline linen shirt';

function text(value) {
  return until(() => evaluate('document.body?.innerText'), (body) => body?.includes(value), 'Missing ' + value);
}

async function control(selector, value, index = 0) {
  const found = await evaluate('(() => { const el = document.querySelectorAll(' + JSON.stringify(selector) + ')[' + index + '];' +
    ' if (!el) return false; const proto = el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;' +
    ' Object.getOwnPropertyDescriptor(proto, "value").set.call(el, ' + JSON.stringify(value) + ');' +
    ' el.dispatchEvent(new Event(el instanceof HTMLSelectElement ? "change" : "input", { bubbles: true })); return true; })()');
  assert.equal(found, true, 'Missing control ' + selector);
}

async function setPhoto() {
  const { root } = await send('DOM.getDocument');
  const { nodeId } = await send('DOM.querySelector', { nodeId: root.nodeId, selector: 'input[type=file]' });
  assert.ok(nodeId, 'Garment file picker is missing');
  await send('DOM.setFileInputFiles', { nodeId, files: [fixture] });
}

function loadedPhoto() {
  return until(() => evaluate('Array.from(document.querySelectorAll(".ward-detail img")).some((img) => img.complete && img.naturalWidth > 0)'),
    Boolean, 'Decrypted garment photo did not render');
}

function detailHeading(name) {
  return until(() => evaluate('document.querySelector(".ward-panel.ward-detail h2")?.innerText'),
    (heading) => heading === name, 'The saved garment detail did not open');
}

try {
  await text('Connected');
  await text('Schema 1 · ' + mode + ' mode');
  await click('Wardrobe');
  if (workflow === 'source') {
    await text('Create your private wardrobe');
    await control('input[type=password]', passphrase, 0);
    await control('input[type=password]', passphrase, 1);
    await click('Create secure wardrobe');
    await text('Your wardrobe is empty');
    await click('Add garment');
    await control('.garment-form input[required]', garmentName);
    await control('.garment-form select', 'casual-shirts');
    await setPhoto();
    await loadedPhoto();
    await click('Review garment');
    await click('Save garment');
    await detailHeading(garmentName);
    await loadedPhoto();
    await click('Edit garment');
    await control('.garment-form input[required]', changedName);
    await click('Review garment');
    await click('Save garment');
    await detailHeading(changedName);
    await loadedPhoto();
    await webview.screenshot('wardrobe-details');
    await click('← Back to wardrobe');
    await text(changedName);
    await control('input[type=search]', 'unmatched');
    await text('No matching garments');
    await click('Clear filters');
    await text(changedName);
    await control('.ward-filters select', 'jeans', 1);
    await text('No matching garments');
    await click('Clear filters');
    await text(changedName);
    await webview.screenshot('wardrobe-grid');
    await click('Backup & recovery');
    await click('Back up to workspace');
    await text('in the workspace backups folder.');
    await click('← Back to wardrobe');
    await text(changedName);
    await click(changedName, true);
    await text('Date added');
    await click('Delete garment');
    await click('Confirm deletion');
    await text('Your wardrobe is empty');
  } else if (workflow === 'restore') {
    await text('Create your private wardrobe');
    await click('Restore an existing backup');
    await until(() => evaluate('Array.from(document.querySelectorAll("select option")).some((item) => item.value === ' + JSON.stringify(fixture) + ')'),
      Boolean, 'The copied backup was unavailable');
    await control('select', fixture);
    await control('input[type=password]', 'a different fixture passphrase');
    await click('Restore backup');
    await text('The passphrase could not unlock');
    await control('input[type=password]', passphrase);
    await click('Restore backup');
    await text(changedName);
    await click(changedName, true);
    await loadedPhoto();
    await webview.screenshot('restored-wardrobe');
  } else if (workflow === 'recover') {
    await text('Unlock your wardrobe');
    await control('input[type=password]', passphrase);
    await click('Unlock wardrobe');
    await text(changedName);
    await click(changedName, true);
    await loadedPhoto();
  } else {
    throw new Error('Unknown wardrobe test workflow');
  }
  webview.assertLocalRequests();
  console.log(mode + ' ' + workflow + ' workflow passed.');
} catch (error) {
  console.error('WebView diagnostic errors:', webview.browserErrors.slice(0, 10));
  console.error('Visible page:', await evaluate('document.body?.innerText'));
  throw error;
} finally {
  webview.close();
}
