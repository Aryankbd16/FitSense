import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import type { DesktopBridge, SystemStatus } from './system';

afterEach(cleanup);

const ready: SystemStatus = {
  worker: { status: 'ready', failure: null, latencyMs: 1 },
  storageMode: 'portable',
  schemaVersion: 1,
  privateStorageLocked: true,
  logsAvailable: true,
};

function makeBridge(): DesktopBridge {
  return {
    status: vi.fn(async () => ready),
    diagnose: vi.fn(async () => ({ pythonVersion: '3.13.17', protocolVersion: 1, responseMs: 0 })),
    retry: vi.fn(async () => {}),
  };
}

describe('FitSense desktop shell', () => {
  it('renders the startup state and then shows the live workspace', async () => {
    render(<App bridge={makeBridge()} />);
    expect(screen.getByRole('heading', { name: 'Your local workspace' })).toBeDefined();
    expect(screen.getByRole('status').textContent).toContain('Connecting');
    expect(await screen.findByText('Connected')).toBeDefined();
    expect(screen.getByText('Schema 1 · portable mode')).toBeDefined();
    expect(screen.getByText('Private storage is locked')).toBeDefined();
  });

  it('opens diagnostics and shows the result of the requested check', async () => {
    const bridge = makeBridge();
    render(<App bridge={bridge} />);
    await screen.findByText('Connected');
    fireEvent.click(screen.getByRole('button', { name: 'Open diagnostics' }));
    expect(within(screen.getByRole('navigation')).getByRole('button', { name: 'Diagnostics' }).getAttribute('aria-current')).toBe('page');
    fireEvent.click(screen.getByRole('button', { name: 'Run diagnostics' }));
    expect(await screen.findByText('3.13.17')).toBeDefined();
    expect(screen.getByText('<1 ms')).toBeDefined();
    expect(bridge.diagnose).toHaveBeenCalledTimes(1);
  });

  it('shows typed worker failures and requests a retry', async () => {
    const bridge = makeBridge();
    bridge.status = async () => ({ ...ready, worker: { status: 'unavailable', failure: 'exited', latencyMs: null } });
    render(<App bridge={bridge} />);
    expect((await screen.findByRole('alert')).textContent).toContain('stopped unexpectedly');
    fireEvent.click(screen.getByRole('button', { name: 'Retry worker' }));
    expect(bridge.retry).toHaveBeenCalledTimes(1);
  });

  it('keeps workspace navigation available while the worker is busy', async () => {
    const bridge = makeBridge();
    bridge.diagnose = () => new Promise(() => {});
    render(<App bridge={bridge} />);
    await screen.findByText('Connected');
    fireEvent.click(screen.getByRole('button', { name: 'Open diagnostics' }));
    fireEvent.click(screen.getByRole('button', { name: 'Run diagnostics' }));
    expect(screen.getByText('Waiting for the worker…')).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Overview' }));
    expect(screen.getByRole('heading', { name: 'Your local workspace' })).toBeDefined();
  });

  it('marks browser mode and disables live diagnostics there', () => {
    render(<App bridge={null} />);
    expect(screen.getByText('Browser preview')).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Open diagnostics' }));
    expect((screen.getByRole('button', { name: 'Run diagnostics' }) as HTMLButtonElement).disabled).toBe(true);
  });
});
