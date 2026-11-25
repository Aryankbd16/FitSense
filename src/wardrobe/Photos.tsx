import { useEffect, useState } from 'react';
import { ImageOff, Shirt } from 'lucide-react';
import { wardrobeError } from './api';
import type { Garment, WardrobeBridge } from './api';

export function GarmentPhoto({ bridge, garment, thumbnail = false }: {
  bridge: WardrobeBridge; garment: Garment; thumbnail?: boolean;
}) {
  const [state, setState] = useState<{ url?: string; error?: string }>({});

  useEffect(() => {
    if (!garment.photo) return;
    let active = true;
    let url: string | undefined;
    setState({});
    bridge.image(garment.id, thumbnail).then((bytes) => {
      if (!active) return;
      url = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }));
      setState({ url });
    }).catch((failure: unknown) => {
      if (active) setState({ error: wardrobeError(failure) });
    });
    return () => {
      active = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [bridge, garment.id, garment.changed, garment.photo, thumbnail]);

  return <div className={'ward-photo' + (thumbnail ? '' : ' ward-photo--detail')}>
    {state.url && garment.photo
      ? <img src={state.url} alt={garment.details.name} />
      : state.error && garment.photo
        ? <span className="ward-photo-placeholder" title={state.error}><ImageOff size={26} aria-hidden="true" />Photo unavailable</span>
        : <span className="ward-photo-placeholder"><Shirt size={27} aria-hidden="true" />
          {garment.photo ? 'Loading photo…' : 'No photo'}
        </span>}
  </div>;
}
