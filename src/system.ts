import { invoke, isTauri } from '@tauri-apps/api/core';

export type WorkerFailure = 'launch' | 'timeout' | 'exited' | 'protocol' | 'worker_error' | 'busy' | 'unavailable';

export interface SystemStatus {
  worker: {
    status: 'starting' | 'ready' | 'unavailable';
    failure: WorkerFailure | null;
    latencyMs: number | null;
  };
  storageMode: 'installed' | 'portable' | 'development';
  schemaVersion: number | null;
  privateStorageLocked: boolean;
  logsAvailable: boolean;
}

export interface DiagnosticReport {
  pythonVersion: string;
  protocolVersion: number;
  responseMs: number;
}

export interface DesktopBridge {
  status(): Promise<SystemStatus>;
  diagnose(): Promise<DiagnosticReport>;
  retry(): Promise<void>;
}

export const desktop: DesktopBridge | null = isTauri() ? {
  status: () => invoke<SystemStatus>('system_status'),
  diagnose: () => invoke<DiagnosticReport>('worker_diagnostics'),
  retry: () => invoke<void>('retry_worker'),
} : null;

export const failureText: Record<WorkerFailure, string> = {
  launch: 'The desktop app could not start the bundled worker.',
  timeout: 'The worker did not respond in time.',
  exited: 'The worker stopped unexpectedly.',
  protocol: 'The worker returned an invalid IPC message.',
  worker_error: 'The worker reported an error.',
  busy: 'The worker is busy. Please try again.',
  unavailable: 'The worker is unavailable. Check its connection and try again.',
};

export function describeFailure(error: unknown): string {
  if (typeof error === 'string' && Object.hasOwn(failureText, error)) {
    return failureText[error as WorkerFailure];
  }
  return 'The desktop app could not complete the request.';
}

export function formatTiming(value: number): string {
  return value === 0 ? '<1 ms' : `${value} ms`;
}
