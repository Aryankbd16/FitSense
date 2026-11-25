import { useEffect, useState } from 'react';
import { HardDrive, Plus } from 'lucide-react';
import { GarmentDetail, GarmentGrid, initialQuery } from './Garments';
import { GarmentForm } from './GarmentForm';
import { VaultAccess, VaultSettings } from './VaultAccess';
import { wardrobeError } from './api';
import type { Category, Garment, VaultState, WardrobeBridge } from './api';
import './wardrobe.css';

type View = { kind: 'grid' | 'add' | 'settings' | 'loading' } | { kind: 'detail' | 'edit'; garment: Garment };

export function Wardrobe({ bridge }: { bridge: WardrobeBridge | null }) {
  const [vault, setVault] = useState<VaultState>();
  const [categories, setCategories] = useState<Category[]>([]);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const [view, setView] = useState<View>({ kind: 'grid' });
  const [query, setQuery] = useState(initialQuery);

  useEffect(() => {
    if (!bridge) return;
    let active = true;
    Promise.all([bridge.state(), bridge.categories()]).then(([state, options]) => {
      if (active) { setCategories(options); setVault(state); }
    }).catch((failure: unknown) => { if (active) setError(wardrobeError(failure)); });
    return () => { active = false; };
  }, [bridge, retry]);

  async function inspect(id: string) {
    if (!bridge) return;
    setView({ kind: 'loading' });
    setError('');
    try { setView({ kind: 'detail', garment: await bridge.detail(id) }); }
    catch (failure) { setError(wardrobeError(failure)); setView({ kind: 'grid' }); }
  }

  function back() { setView({ kind: 'grid' }); setError(''); }

  function afterDelete() {
    setQuery({ ...query, offset: 0 });
    back();
  }

  function refreshStatus() {
    if (bridge) {
      void bridge.state().then(setVault).catch((failure: unknown) => setError(wardrobeError(failure)));
    }
  }

  return <section aria-labelledby="wardrobe-heading">
    <div className="page-heading wardrobe-heading">
      <div><h1 id="wardrobe-heading">Wardrobe</h1>
        <p>Organize your garments and photos in an encrypted workspace on this device.</p></div>
      {vault?.state === 'open' && view.kind === 'grid' && <div className="ward-actions">
        <button className="button ward-secondary" onClick={() => setView({ kind: 'settings' })}><HardDrive size={17} aria-hidden="true" />Backup &amp; recovery</button>
        <button className="button button--dark" onClick={() => setView({ kind: 'add' })}><Plus size={17} aria-hidden="true" />Add garment</button>
      </div>}
    </div>
    {!bridge && <p className="notice">Open the FitSense desktop app to manage your encrypted wardrobe.</p>}
    {bridge && !vault && !error && <p className="ward-panel" role="status">Checking secure storage…</p>}
    {error && <div className="notice notice--error ward-actions" role="alert">
      {error}
      {!vault && <button className="button ward-secondary" onClick={() => { setError(''); setRetry((value) => value + 1); }}>Try again</button>}
    </div>}
    {bridge && vault && vault.state !== 'open' && <VaultAccess bridge={bridge} state={vault.state} onOpen={setVault} />}
    {bridge && vault?.state === 'open' && <>
      {vault.cleanupPending && <p className="notice">Some unreferenced encrypted files are awaiting cleanup.</p>}
      {view.kind === 'grid' && <GarmentGrid bridge={bridge} categories={categories} query={query} setQuery={setQuery}
        add={() => setView({ kind: 'add' })} open={(id) => void inspect(id)} />}
      {view.kind === 'loading' && <p className="ward-panel" role="status">Opening garment…</p>}
      {view.kind === 'detail' && <GarmentDetail bridge={bridge} garment={view.garment} categories={categories}
        onBack={back} onEdit={() => setView({ kind: 'edit', garment: view.garment })}
        onDelete={() => { afterDelete(); refreshStatus(); }} />}
      {(view.kind === 'add' || view.kind === 'edit') && <GarmentForm bridge={bridge} categories={categories}
        garment={view.kind === 'edit' ? view.garment : undefined}
        onBack={() => view.kind === 'edit' ? setView({ kind: 'detail', garment: view.garment }) : back()}
        onSaved={(garment) => { setView({ kind: 'detail', garment }); refreshStatus(); }} />}
      {view.kind === 'settings' && <VaultSettings bridge={bridge} onBack={back} />}
    </>}
  </section>;
}
