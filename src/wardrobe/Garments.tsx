import { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, Plus, Search, Shirt } from 'lucide-react';
import { CategoryOptions } from './GarmentForm';
import { GarmentPhoto } from './Photos';
import { wardrobeError } from './api';
import type { Category, Garment, Page, Query, WardrobeBridge } from './api';

const pageSize = 24;
export const initialQuery: Query = { search: '', category: '', group: '', sort: 'newest', offset: 0 };

export function GarmentGrid({ bridge, categories, query, setQuery, add, open }: {
  bridge: WardrobeBridge; categories: Category[]; query: Query; setQuery(query: Query): void;
  add(): void; open(id: string): void;
}) {
  const [page, setPage] = useState<Page>();
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);

  useEffect(() => {
    let active = true;
    setPage(undefined);
    setError('');
    const timer = setTimeout(() => {
      bridge.list(query).then((result) => { if (active) setPage(result); })
        .catch((failure: unknown) => { if (active) setError(wardrobeError(failure)); });
    }, 160);
    return () => { active = false; clearTimeout(timer); };
  }, [bridge, query, retry]);

  const filtered = Boolean(query.search.trim() || query.category || query.group);
  const detail = (id: string) => categories.find((category) => category.id === id);
  return <div className="ward-stack">
    <div className="ward-panel ward-filters">
      <label>Search garments<span className="ward-search"><Search size={17} aria-hidden="true" />
        <input type="search" value={query.search} maxLength={120} placeholder="Search by name, colour or fabric"
          onChange={(event) => setQuery({ ...query, search: event.target.value, offset: 0 })} /></span></label>
      <label>Group<select value={query.group}
        onChange={(event) => setQuery({ ...query, group: event.target.value, category: '', offset: 0 })}>
        <option value="">All groups</option>
        {['Upper wear', 'Lower wear', 'Outerwear', 'Accessories'].map((group) => <option key={group}>{group}</option>)}
      </select></label>
      <label>Category<select value={query.category}
        onChange={(event) => setQuery({ ...query, category: event.target.value, offset: 0 })}>
        <option value="">All categories</option>
        <CategoryOptions categories={categories.filter((category) => !query.group || category.group === query.group)} />
      </select></label>
      <label>Sort<select value={query.sort}
        onChange={(event) => setQuery({ ...query, sort: event.target.value as Query['sort'], offset: 0 })}>
        <option value="newest">Newest first</option><option value="oldest">Oldest first</option><option value="name">Name A–Z</option>
      </select></label>
    </div>
    {error && <div className="ward-panel ward-actions">
      <p className="ward-error" role="alert">{error}</p>
      <button className="button ward-secondary" onClick={() => setRetry((value) => value + 1)}>Try again</button>
    </div>}
    {page === undefined && !error && <p className="ward-panel" role="status">Loading wardrobe…</p>}
    {page?.garments.length === 0 && <section className="ward-empty">
      <span className="ward-empty-icon"><Shirt size={33} aria-hidden="true" /></span>
      <div><h2>{filtered ? 'No matching garments' : 'Your wardrobe is empty'}</h2>
        <p>{filtered ? 'Try a different search or clear the filters.' : 'Add your first garment and keep its details and photo together.'}</p>
        {filtered
          ? <button className="button ward-secondary" onClick={() => setQuery(initialQuery)}>Clear filters</button>
          : <button className="button button--light" onClick={add}><Plus size={17} aria-hidden="true" />Add your first garment</button>}
      </div>
    </section>}
    {page && page.garments.length > 0 && <div className="ward-grid" aria-label="Garments">
      {page.garments.map((garment) => <button className="ward-card" key={garment.id} onClick={() => open(garment.id)}>
        <GarmentPhoto bridge={bridge} garment={garment} thumbnail />
        <span className="ward-card-caption">
          <small>{detail(garment.details.category)?.label}</small>
          <strong>{garment.details.name}</strong>
          {garment.details.primaryColour && <span>{garment.details.primaryColour}</span>}
        </span>
      </button>)}
    </div>}
    {(query.offset > 0 || page?.hasMore) && <div className="ward-actions ward-pagination">
      <button className="button ward-secondary" disabled={!page || query.offset === 0}
        onClick={() => setQuery({ ...query, offset: Math.max(0, query.offset - pageSize) })}><ChevronLeft size={17} /> Previous</button>
      <button className="button ward-secondary" disabled={!page?.hasMore}
        onClick={() => setQuery({ ...query, offset: query.offset + pageSize })}>Next <ChevronRight size={17} /></button>
    </div>}
  </div>;
}

export function GarmentDetail({ bridge, garment, categories, onBack, onEdit, onDelete }: {
  bridge: WardrobeBridge; garment: Garment; categories: Category[];
  onBack(): void; onEdit(): void; onDelete(): void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const { details } = garment;
  const category = categories.find((category) => category.id === details.category);
  const fields = [
    ['Primary colour', details.primaryColour], ['Secondary colours', details.secondaryColours.join(', ')],
    ['Pattern', details.pattern], ['Fit', details.fit], ['Sleeve type', details.sleeve],
    ['Fabric', details.fabric], ['Formality', details.formality], ['Season suitability', details.seasons.join(', ')],
    ['Date added', new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(garment.added))],
    ['Notes', details.notes],
  ];

  async function remove() {
    if (busy) return;
    setError('');
    setBusy(true);
    try { await bridge.delete(garment.id); onDelete(); }
    catch (failure) { setError(wardrobeError(failure)); }
    finally { setBusy(false); }
  }

  return <div className="ward-stack">
    <button className="ward-link" disabled={busy} onClick={onBack}>← Back to wardrobe</button>
    <section className="ward-panel ward-detail">
      <GarmentPhoto bridge={bridge} garment={garment} />
      <div className="ward-stack">
        <div><p className="ward-hint">{category?.group} / {category?.label}</p><h2>{details.name}</h2></div>
        <dl className="ward-meta">{fields.map(([label, value]) => value && <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
        {!confirming && <div className="ward-actions">
          <button className="button button--dark" onClick={onEdit}>Edit garment</button>
          <button className="button ward-secondary ward-danger" onClick={() => setConfirming(true)}>Delete garment</button>
        </div>}
        {confirming && <div className="notice notice--error ward-confirm">
          <p>Delete this garment and its encrypted photo files?</p>
          <div className="ward-actions">
            <button className="button ward-secondary" disabled={busy} onClick={() => setConfirming(false)}>Keep garment</button>
            <button className="button ward-delete" disabled={busy} onClick={() => void remove()}>{busy ? 'Deleting…' : 'Confirm deletion'}</button>
          </div>
        </div>}
        {error && <p className="ward-error" role="alert">{error}</p>}
      </div>
    </section>
  </div>;
}
