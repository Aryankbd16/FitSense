import { useState } from 'react';
import type { ElementType } from 'react';
import { Activity, ArrowDownRight, ArrowUpRight, CircleHelp, Database, FileText, LayoutGrid, LockKeyhole, Radio, RefreshCw, ShieldCheck, Shirt, Terminal } from 'lucide-react';
import { desktop, describeFailure, failureText, formatTiming } from './system';
import type { DesktopBridge, DiagnosticReport, SystemStatus } from './system';
import { useSystem } from './useSystem';
import { wardrobe } from './wardrobe/api';
import type { WardrobeBridge } from './wardrobe/api';
import { Wardrobe } from './wardrobe/Wardrobe';

const tabs = { overview: 'Overview', wardrobe: 'Wardrobe', diagnostics: 'Diagnostics' } as const;
type Page = keyof typeof tabs;
type Tone = 'positive' | 'pending' | 'error' | 'muted';

function StatusPill({ text, tone }: { text: string; tone: Tone }) {
  return <span className={`pill pill--${tone}`}><span className="dot" />{text}</span>;
}

function workerStatus(worker?: SystemStatus['worker']): { text: string; tone: Tone } {
  switch (worker?.status) {
    case 'ready': return { text: 'Connected', tone: 'positive' };
    case 'starting': return { text: 'Starting', tone: 'pending' };
    case 'unavailable': return { text: 'Unavailable', tone: 'error' };
    default: return { text: 'Awaiting desktop', tone: 'muted' };
  }
}

function StatusCard({ title, icon: Icon, status, tone, detail }: {
  title: string;
  icon: ElementType;
  status: string;
  tone: Tone;
  detail: string;
}) {
  return <article className="status-card">
    <div className="status-card__top"><span className="icon-tile"><Icon size={20} aria-hidden="true" /></span><StatusPill text={status} tone={tone} /></div>
    <h3>{title}</h3>
    <p>{detail}</p>
  </article>;
}

function WorkspaceStatus({ status }: { status?: SystemStatus }) {
  const worker = workerStatus(status?.worker);
  const schema = status?.schemaVersion;
  const heartbeat = status?.worker.latencyMs;
  return <section className="workspace" aria-labelledby="workspace-title">
    <div className="section-header"><div><h2 id="workspace-title">Workspace status</h2><p>Live checks from this device.</p></div><Radio size={20} aria-hidden="true" /></div>
    <div className="status-grid">
      <StatusCard title="Local worker" icon={Activity} status={worker.text} tone={worker.tone}
        detail={heartbeat == null ? 'Python process and IPC health' : `Latest health check · ${formatTiming(heartbeat)}`} />
      <StatusCard title="SQLite storage" icon={Database} status={status ? (schema == null ? 'Unavailable' : 'Ready') : 'Pending'}
        tone={status ? (schema == null ? 'error' : 'positive') : 'muted'}
        detail={schema == null ? 'Local database and workspace folders' : `Schema ${schema} · ${status?.storageMode ?? ''} mode`} />
      <StatusCard title="Local logs" icon={FileText} status={status ? (status.logsAvailable ? 'Active' : 'Degraded') : 'Pending'}
        tone={status ? (status.logsAvailable ? 'positive' : 'error') : 'muted'} detail="Worker and desktop diagnostics" />
    </div>
  </section>;
}

function Diagnostics({ bridge, status }: { bridge: DesktopBridge | null; status?: SystemStatus }) {
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function runCheck() {
    if (!bridge || pending) return;
    setPending(true);
    setError(null);
    setReport(null);
    try {
      setReport(await bridge.diagnose());
    } catch (failure) {
      setError(describeFailure(failure));
    } finally {
      setPending(false);
    }
  }

  return <section className="diagnostic-panel" aria-labelledby="diagnostic-heading">
    <div className="diagnostic-panel__heading">
      <div><span className="icon-tile"><Terminal size={21} aria-hidden="true" /></span>
        <h2 id="diagnostic-heading">Worker diagnostics</h2><p>Send a diagnostic request to the local Python worker.</p></div>
      <button className="button button--dark" disabled={pending || status?.worker.status !== 'ready'} onClick={() => void runCheck()}>
        {pending ? 'Running…' : 'Run diagnostics'} <ArrowUpRight size={17} aria-hidden="true" />
      </button>
    </div>
    {error && <p role="alert" className="diagnostic-error">{error}</p>}
    <div aria-live="polite">
      {report ? <dl className="diagnostic-results">
        <div><dt>Python</dt><dd>{report.pythonVersion}</dd></div>
        <div><dt>IPC protocol</dt><dd>v{report.protocolVersion}</dd></div>
        <div><dt>Response time</dt><dd>{formatTiming(report.responseMs)}</dd></div>
      </dl> : <p className="diagnostic-empty">{pending ? 'Waiting for the worker…' : 'No diagnostics run in this view.'}</p>}
    </div>
  </section>;
}

export function App({ bridge = desktop, wardrobeBridge = wardrobe }: {
  bridge?: DesktopBridge | null;
  wardrobeBridge?: WardrobeBridge | null;
}) {
  const [page, setPage] = useState<Page>('overview');
  const [retryPending, setRetryPending] = useState(false);
  const [retryError, setRetryError] = useState<string | null>(null);
  const connection = useSystem(bridge);
  const status = connection.state === 'connected' ? connection.status : undefined;
  const failure = status?.worker.failure;

  async function retry() {
    if (!bridge || retryPending) return;
    setRetryError(null);
    setRetryPending(true);
    try { await bridge.retry(); } catch (error) { setRetryError(describeFailure(error)); }
    finally { setRetryPending(false); }
  }

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark" aria-hidden="true"><i /><i /></span><span>FitSense<span className="brand-period">.</span></span></div>
      <nav aria-label="Workspace">
        <span className="nav-label">Workspace</span>
        <button aria-current={page === 'overview' ? 'page' : undefined} onClick={() => setPage('overview')}><LayoutGrid size={18} aria-hidden="true" />Overview</button>
        <button aria-current={page === 'wardrobe' ? 'page' : undefined} onClick={() => setPage('wardrobe')}><Shirt size={18} aria-hidden="true" />Wardrobe</button>
        <button aria-current={page === 'diagnostics' ? 'page' : undefined} onClick={() => setPage('diagnostics')}><Activity size={18} aria-hidden="true" />Diagnostics</button>
      </nav>
      <div className="sidebar-foot"><span className="sidebar-foot__icon"><ShieldCheck size={17} aria-hidden="true" /></span><span><strong>On this device</strong><small>FitSense desktop</small></span></div>
    </aside>

    <div className="main-column">
      <header className="topbar"><div><span>Workspace</span><span aria-hidden="true">/</span><strong>{tabs[page]}</strong></div>
        <span className="topbar__mode"><span className="dot" />{bridge ? 'Local mode' : 'Browser preview'}</span></header>
      <main>
        {page === 'wardrobe' ? <Wardrobe bridge={wardrobeBridge} /> : <>
        <div className="page-heading">
          <h1>{page === 'overview' ? 'Your local workspace' : 'Diagnostics'}</h1>
          <p>{page === 'overview' ? 'Manage the local services behind your FitSense wardrobe.' : 'Check the connection between the desktop app and its worker.'}</p>
        </div>

        {connection.state !== 'connected' && <div className="notice" role={connection.state === 'disconnected' ? 'alert' : 'status'}>
          <CircleHelp size={18} aria-hidden="true" />
          {connection.state === 'preview' ? 'Open the FitSense desktop app to connect to the worker and local storage.' : connection.state === 'connecting' ? 'Connecting to the desktop app…' : 'The desktop connection is unavailable. FitSense will check again automatically.'}
        </div>}

        {failure && <div className="notice notice--error" role="alert"><Activity size={18} aria-hidden="true" />
          <span>{retryError ?? failureText[failure]}</span><button className="retry" disabled={retryPending} onClick={() => void retry()}><RefreshCw size={15} aria-hidden="true" />{retryPending ? 'Retrying…' : 'Retry worker'}</button>
        </div>}

        {page === 'overview' ? <section className="hero" aria-labelledby="hero-heading">
          <div className="hero__art" aria-hidden="true"><div /><div /><div /><span /><span /></div>
          <div className="hero__content"><div className="hero__label"><span className="dot" />Offline-first desktop</div>
            <h2 id="hero-heading">Offline by design.</h2>
            <p>FitSense runs its worker and stores its workspace on this device. No account or internet connection required.</p>
            <button className="button button--light" onClick={() => setPage('diagnostics')}>Open diagnostics <ArrowUpRight size={17} aria-hidden="true" /></button>
          </div>
          <ArrowDownRight className="hero__corner" size={29} aria-hidden="true" />
        </section> : <Diagnostics bridge={bridge} status={status} />}

        <WorkspaceStatus status={status} />
        {status?.privateStorageLocked && <section className="privacy-note" aria-label="Private storage">
          <span className="icon-tile"><LockKeyhole size={18} aria-hidden="true" /></span>
          <div><h2>Private storage is locked</h2><p>Open Wardrobe to set up or unlock encrypted storage.</p></div>
          <button className="ward-link" onClick={() => setPage('wardrobe')}>Open wardrobe</button>
        </section>}
        </>}
      </main>
    </div>
  </div>;
}
