import { useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { Check, ImagePlus } from 'lucide-react';
import { emptyGarment, filePhoto, wardrobeError } from './api';
import type { Category, Garment, GarmentInput, PhotoChange, WardrobeBridge } from './api';
import { GarmentPhoto } from './Photos';

const groups = ['Upper wear', 'Lower wear', 'Outerwear', 'Accessories'];
const seasons = ['Spring', 'Summer', 'Autumn', 'Winter'];
const optional: [keyof Pick<GarmentInput, 'primaryColour' | 'pattern' | 'fit' | 'sleeve' | 'fabric' | 'formality'>, string][] = [
  ['primaryColour', 'Primary colour'], ['pattern', 'Pattern'], ['fit', 'Fit'],
  ['sleeve', 'Sleeve type'], ['fabric', 'Fabric'], ['formality', 'Formality'],
];

export function CategoryOptions({ categories }: { categories: Category[] }) {
  return groups.map((group) => <optgroup key={group} label={group}>
    {categories.filter((category) => category.group === group)
      .map((category) => <option key={category.id} value={category.id}>{category.label}</option>)}
  </optgroup>);
}

export function GarmentForm({ bridge, categories, garment, onBack, onSaved }: {
  bridge: WardrobeBridge; categories: Category[]; garment?: Garment;
  onBack(): void; onSaved(garment: Garment): void;
}) {
  const [input, setInput] = useState(garment?.details ?? emptyGarment);
  const [colours, setColours] = useState(input.secondaryColours.join(', '));
  const [file, setFile] = useState<File>();
  const [remove, setRemove] = useState(false);
  const [preview, setPreview] = useState('');
  const [review, setReview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const category = categories.find((option) => option.id === input.category);

  useEffect(() => {
    if (!file) return;
    const url = URL.createObjectURL(file);
    setPreview(url);
    return () => URL.revokeObjectURL(url);
  }, [file]);

  function setField<K extends keyof GarmentInput>(field: K, value: GarmentInput[K]) {
    setInput((existing) => ({ ...existing, [field]: value }));
  }

  function chooseFile(file?: File) {
    if (!file) return;
    if (file.size === 0 || file.size > 12 * 1024 * 1024) { setError(wardrobeError('invalid_image')); return; }
    setError('');
    setFile(file);
    setRemove(false);
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setError('');
    const name = input.name.trim();
    const secondaryColours = colours.split(',').map((item) => item.trim()).filter(Boolean);
    if (!name || !category || secondaryColours.length > 8) {
      setError('Enter a garment name and category. Use up to eight secondary colours.');
      return;
    }
    if (!review) { setReview(true); return; }
    setBusy(true);
    try {
      const photo: PhotoChange = file ? await filePhoto(file) : { kind: remove ? 'remove' : 'keep' };
      onSaved(await bridge.save(garment?.id ?? null, { ...input, name, secondaryColours }, photo));
    } catch (failure) {
      setError(wardrobeError(failure));
    } finally {
      setBusy(false);
    }
  }

  const photo = file && preview
    ? <div className="ward-photo ward-photo--detail"><img src={preview} alt="Selected garment preview" /></div>
    : garment?.photo && !remove
      ? <GarmentPhoto bridge={bridge} garment={garment} />
      : <div className="ward-photo ward-photo--detail"><span className="ward-photo-placeholder">
        <ImagePlus size={25} aria-hidden="true" />No photo selected
      </span></div>;

  return <div className="ward-stack">
    <button className="ward-link" disabled={busy} onClick={onBack}>← {garment ? 'Back to garment' : 'Back to wardrobe'}</button>
    <section className="ward-panel">
      <h2>{review ? 'Review garment' : garment ? 'Edit garment' : 'Add a garment'}</h2>
      <form className="garment-form ward-stack" onSubmit={(event) => void submit(event)}>
        {review ? <div className="ward-detail">
          {photo}
          <div><h3>{input.name.trim()}</h3>
            <p>{category?.group} / {category?.label}</p>
            <dl className="ward-meta">
              {optional.map(([field, label]) => input[field] && <div key={field}><dt>{label}</dt><dd>{input[field]}</dd></div>)}
              {colours.trim() && <div><dt>Secondary colours</dt><dd>{colours}</dd></div>}
              {input.seasons.length > 0 && <div><dt>Season</dt><dd>{input.seasons.join(', ')}</dd></div>}
              {input.notes && <div><dt>Notes</dt><dd>{input.notes}</dd></div>}
            </dl>
          </div>
        </div> : <div className="ward-detail">
          <div className="ward-stack">
            {photo}
            <label>Garment photo <span className="ward-hint">(optional)</span>
              <input type="file" accept="image/jpeg,image/png,.jpg,.jpeg,.png"
                onChange={(event) => chooseFile(event.target.files?.[0])} />
            </label>
            <p className="ward-hint">JPEG or PNG up to 12 MB. FitSense preserves the original.</p>
            {(file || (garment?.photo && !remove)) && <button className="ward-link" type="button" onClick={() => {
              setFile(undefined); setRemove(true);
            }}>Remove photo</button>}
          </div>
          <div className="ward-stack">
            <div className="ward-fields">
              <label>Garment name<input required maxLength={120} value={input.name} onChange={(event) => setField('name', event.target.value)} /></label>
              <label>Category and subcategory
                <select required value={input.category} onChange={(event) => setField('category', event.target.value)}>
                  <option value="">Choose a category</option><CategoryOptions categories={categories} />
                </select>
              </label>
              {optional.map(([field, label]) => <label key={field}>{label}
                <input maxLength={120} value={input[field]} onChange={(event) => setField(field, event.target.value)} />
              </label>)}
            </div>
            <label>Secondary colours<input maxLength={500} value={colours} placeholder="Separate colours with commas"
              onChange={(event) => setColours(event.target.value)} /></label>
            <fieldset className="ward-seasons"><legend>Season suitability</legend>
              {seasons.map((season) => <label key={season}><input type="checkbox" checked={input.seasons.includes(season)}
                onChange={(event) => setField('seasons', event.target.checked
                  ? [...input.seasons, season] : input.seasons.filter((value) => value !== season))} />{season}</label>)}
            </fieldset>
            <label>Notes<textarea rows={3} maxLength={4000} value={input.notes} onChange={(event) => setField('notes', event.target.value)} /></label>
          </div>
        </div>}
        {error && <p className="ward-error" role="alert">{error}</p>}
        <div className="ward-actions">
          {review && <button type="button" className="button ward-secondary" disabled={busy} onClick={() => setReview(false)}>Back to edit</button>}
          <button className="button button--dark" type="submit" disabled={busy}>
            {review && <Check size={16} aria-hidden="true" />}
            {busy ? 'Saving securely…' : review ? 'Save garment' : 'Review garment'}
          </button>
        </div>
      </form>
    </section>
  </div>;
}
