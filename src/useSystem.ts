import { useEffect, useState } from 'react';
import type { DesktopBridge, SystemStatus } from './system';

type Connection =
  | { state: 'connecting' | 'preview' | 'disconnected' }
  | { state: 'connected'; status: SystemStatus };

export function useSystem(bridge: DesktopBridge | null): Connection {
  const [connection, setConnection] = useState<Connection>({ state: bridge ? 'connecting' : 'preview' });

  useEffect(() => {
    if (!bridge) return;
    let timer: ReturnType<typeof setTimeout>;
    let stopped = false;

    async function poll(client: DesktopBridge) {
      try {
        const status = await client.status();
        if (!stopped) setConnection({ state: 'connected', status });
      } catch {
        if (!stopped) setConnection({ state: 'disconnected' });
      } finally {
        if (!stopped) timer = setTimeout(() => void poll(client), 1000);
      }
    }

    void poll(bridge);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [bridge]);

  return connection;
}
