import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export async function until(read, condition, description, timeout = 25_000) {
  const deadline = Date.now() + timeout;
  let value;
  do {
    value = await read();
    if (condition(value)) return value;
    await delay(120);
  } while (Date.now() < deadline);
  throw new Error(description + ': ' + JSON.stringify(value));
}

export async function openWebview(port, screenshots) {
  const endpoint = 'http://127.0.0.1:' + port + '/json/list';
  const { page } = await until(async () => {
    try {
      const response = await fetch(endpoint, { signal: AbortSignal.timeout(500) });
      const targets = await response.json();
      return {
        page: targets.find((target) => target.type === 'page' && /^https?:\/\/tauri\.localhost(?:[/:]|$)/.test(target.url)),
        urls: targets.map((target) => target.url),
      };
    } catch (error) {
      return { endpointError: error.cause?.code ?? error.message };
    }
  }, ({ page }) => Boolean(page), 'FitSense WebView2 target was unavailable');

  const socket = new WebSocket(page.webSocketDebuggerUrl);
  const requests = new Set();
  const browserErrors = [];
  const pending = new Map();
  let nextId = 0;
  socket.addEventListener('message', ({ data }) => {
    const packet = JSON.parse(data);
    if (packet.method === 'Network.requestWillBeSent') requests.add(packet.params.request.url);
    if (packet.method === 'Runtime.exceptionThrown') browserErrors.push(packet.params.exceptionDetails);
    if (packet.method === 'Log.entryAdded' && packet.params.entry.level === 'error') browserErrors.push(packet.params.entry.text);
    if (packet.method === 'Network.loadingFailed') browserErrors.push(packet.params.errorText);
    if (packet.method === 'Network.responseReceived' && packet.params.response.status >= 400) {
      browserErrors.push(packet.params.response.status + ' ' + packet.params.response.url);
    }
    if (packet.id !== undefined) {
      const resolver = pending.get(packet.id);
      if (!resolver) return;
      pending.delete(packet.id);
      clearTimeout(resolver.timer);
      if (packet.error) resolver.reject(new Error(packet.error.message));
      else resolver.resolve(packet.result);
    }
  });

  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++nextId;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(method + ' timed out'));
      }, 5000);
      pending.set(id, { resolve, reject, timer });
      socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async function evaluate(expression) {
    const result = await send('Runtime.evaluate', { expression, returnByValue: true });
    assert.equal(result.exceptionDetails, undefined, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }

  async function click(name, contains = false) {
    const found = await evaluate('(() => { const button = Array.from(document.querySelectorAll("button")).find((el) => ' +
      (contains ? 'el.innerText.includes(' + JSON.stringify(name) + ')' : 'el.innerText === ' + JSON.stringify(name)) +
      '); if (!button || button.disabled) return false; button.click(); return true; })()');
    assert.equal(found, true, 'button ' + name + ' unavailable');
  }

  async function screenshot(name) {
    const { data } = await send('Page.captureScreenshot', { format: 'png' });
    await writeFile(join(screenshots, name + '.png'), Buffer.from(data, 'base64'));
  }

  function assertLocalRequests() {
    assert.ok(requests.size > 0, 'The page reload did not produce any observable requests');
    const external = [...requests].filter((url) => !/^(?:(?:blob:)?https?:\/\/(?:tauri|ipc)\.localhost(?:[/:]|$)|ipc:)/.test(url));
    assert.equal(external.length, 0, 'Unexpected WebView requests: ' + external.map((url) => url.slice(0, 120)).join(', '));
    assert.ok(!browserErrors.some((error) => typeof error === 'string' && error.includes('Content Security Policy')),
      'A resource violated the release content security policy');
  }

  try {
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true });
      socket.addEventListener('error', reject, { once: true });
      AbortSignal.timeout(3000).addEventListener('abort', () => reject(new Error('WebView2 connection timed out')), { once: true });
    });
    await Promise.all([send('Network.enable'), send('Log.enable'), send('Runtime.enable')]);
    await until(() => evaluate('document.readyState === "complete" && Boolean(document.getElementById("root"))'),
      Boolean, 'Initial application document was unavailable');
    await send('Page.reload', { ignoreCache: true });
    return { send, evaluate, click, screenshot, assertLocalRequests, requests, browserErrors, close: () => socket.close() };
  } catch (error) {
    socket.close();
    throw error;
  }
}
