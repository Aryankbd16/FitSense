import { useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { HardDrive, KeyRound, LockKeyhole, ShieldCheck } from 'lucide-react';
import { wardrobeError } from './api';
import type { VaultState, WardrobeBridge } from './api';

function Passphrase({ label, value, update, current = false }: {
  label: string; value: string; update(value: string): void; current?: boolean;
}) {
  return <label>{label}
    <input type="password" value={value} onChange={(event) => update(event.target.value)}
      required minLength={12} maxLength={256} autoComplete={current ? 'current-password' : 'new-password'} />
  </label>;
}

export function VaultAccess({ bridge, state, onOpen }: {
  bridge: WardrobeBridge; state: 'setup' | 'recovery'; onOpen(state: VaultState): void;
}) {
  const [restoring, setRestoring] = useState(false);
  const [names, setNames] = useState<string[]>([]);
  const [name, setName] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [confirm, setConfirm] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const setup = state === 'setup' && !restoring;

  useEffect(() => {
    if (!restoring) return;
    let active = true;
    bridge.backups().then((next) => { if (active) setNames(next); })
      .catch((failure: unknown) => { if (active) setError(wardrobeError(failure)); });
    return () => { active = false; };
  }, [bridge, restoring]);

  function switchMode(next: boolean) {
    setRestoring(next);
    setPassphrase('');
    setConfirm('');
    setError('');
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    if (setup && passphrase !== confirm) { setError('The two passphrases must match.'); return; }
    setBusy(true);
    setError('');
    try {
      const status = restoring ? await bridge.restore(passphrase, name || null)
        : setup ? await bridge.setup(passphrase) : await bridge.unlock(passphrase);
      setPassphrase('');
      setConfirm('');
      onOpen(status);
    } catch (failure) {
      setError(wardrobeError(failure));
    } finally {
      setBusy(false);
    }
  }

  return <section className="ward-panel vault-panel">
    <span className="icon-tile"><LockKeyhole size={20} aria-hidden="true" /></span>
    <h2>{restoring ? 'Restore your wardrobe' : setup ? 'Create your private wardrobe' : 'Unlock your wardrobe'}</h2>
    <p>{restoring ? 'Use a backup and the passphrase you had when you created it.'
      : setup ? 'Keep a recovery passphrase to open a copied wardrobe or restore an encrypted backup.'
        : 'Enter your recovery passphrase to open the wardrobe for this Windows account.'}</p>
    <form onSubmit={(event) => void submit(event)}>
      <fieldset disabled={busy}>
        {restoring && <label>Backup
          <select value={name} onChange={(event) => setName(event.target.value)}>
            <option value="">Choose a backup file…</option>
            {names.map((filename) => <option key={filename} value={filename}>{filename}</option>)}
          </select>
        </label>}
        <Passphrase label="Recovery passphrase" value={passphrase} update={setPassphrase} current={!setup} />
        {setup && <Passphrase label="Confirm recovery passphrase" value={confirm} update={setConfirm} />}
        {setup && <p className="ward-hint">Use a long, unique phrase. Keep it somewhere safe outside FitSense.</p>}
        <button className="button button--dark" type="submit">
          {busy ? 'Working…' : restoring ? 'Restore backup' : setup ? 'Create secure wardrobe' : 'Unlock wardrobe'}
        </button>
      </fieldset>
      {busy && <p role="status">Verifying encrypted storage…</p>}
      {error && <p className="ward-error" role="alert">{error}</p>}
    </form>
    {restoring && busy && <button className="button ward-secondary" onClick={() => void bridge.cancel()}>Cancel restore</button>}
    {state === 'setup' && !busy && <button className="ward-link" onClick={() => switchMode(!restoring)}>
      {restoring ? 'Back to wardrobe setup' : 'Restore an existing backup'}
    </button>}
  </section>;
}

export function VaultSettings({ bridge, onBack }: { bridge: WardrobeBridge; onBack(): void }) {
  const [backup, setBackup] = useState(false);
  const [changing, setChanging] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [current, setCurrent] = useState('');
  const [replacement, setReplacement] = useState('');
  const [confirm, setConfirm] = useState('');

  async function exportBackup(choose: boolean) {
    setError('');
    setMessage('');
    setBackup(true);
    try {
      const name = await bridge.backup(choose);
      setMessage('Saved ' + name + (choose ? '.' : ' in the workspace backups folder.'));
    } catch (failure) {
      setError(wardrobeError(failure));
    } finally {
      setBackup(false);
    }
  }

  async function changePassphrase(event: FormEvent) {
    event.preventDefault();
    setError('');
    setMessage('');
    if (replacement !== confirm) { setError('The two new passphrases must match.'); return; }
    setChanging(true);
    try {
      await bridge.changePassphrase(current, replacement);
      setCurrent('');
      setReplacement('');
      setConfirm('');
      setMessage('Recovery passphrase updated. Existing backups still use their original passphrase.');
    } catch (failure) {
      setError(wardrobeError(failure));
    } finally {
      setChanging(false);
    }
  }

  return <div className="ward-stack">
    <button className="ward-link" onClick={onBack}>← Back to wardrobe</button>
    <section className="ward-panel vault-panel">
      <span className="icon-tile"><HardDrive size={20} aria-hidden="true" /></span>
      <h2>Encrypted backup</h2>
      <p>Save garment details and all associated photos in one backup. A backup uses the passphrase in effect when you create it.</p>
      <div className="ward-actions">
        <button className="button button--dark" disabled={backup || changing} onClick={() => void exportBackup(false)}>Back up to workspace</button>
        <button className="button ward-secondary" disabled={backup || changing} onClick={() => void exportBackup(true)}>Export to another folder…</button>
        {backup && <button className="button ward-secondary" onClick={() => void bridge.cancel()}>Cancel backup</button>}
      </div>
      {backup && <p role="status">Preparing your encrypted backup…</p>}
      <p className="ward-hint"><ShieldCheck size={16} aria-hidden="true" />Restore from wardrobe setup in a fresh workspace.</p>
    </section>
    <section className="ward-panel vault-panel">
      <span className="icon-tile"><KeyRound size={20} aria-hidden="true" /></span>
      <h2>Change recovery passphrase</h2>
      <form onSubmit={(event) => void changePassphrase(event)}>
        <fieldset disabled={backup || changing}>
          <Passphrase label="Current passphrase" value={current} update={setCurrent} current />
          <Passphrase label="New passphrase" value={replacement} update={setReplacement} />
          <Passphrase label="Confirm new passphrase" value={confirm} update={setConfirm} />
          <button className="button button--dark" type="submit">{changing ? 'Updating…' : 'Update passphrase'}</button>
        </fieldset>
      </form>
    </section>
    {message && <p className="notice" role="status">{message}</p>}
    {error && <p className="ward-error" role="alert">{error}</p>}
  </div>;
}
