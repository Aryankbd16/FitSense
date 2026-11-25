import { invoke, isTauri } from '@tauri-apps/api/core';

export type VaultState = { state: 'setup' | 'recovery' } | { state: 'open'; cleanupPending: boolean };
export interface Category { id: string; group: string; label: string }
export interface GarmentInput {
  name: string;
  category: string;
  primaryColour: string;
  secondaryColours: string[];
  pattern: string;
  fit: string;
  sleeve: string;
  fabric: string;
  formality: string;
  seasons: string[];
  notes: string;
}
export interface Garment { id: string; added: number; changed: number; photo: boolean; details: GarmentInput }
export type PhotoChange = { kind: 'keep' | 'remove' } | { kind: 'upload'; base64: string };
export interface Query { search: string; category: string; group: string; sort: 'newest' | 'oldest' | 'name'; offset: number }
export interface Page { garments: Garment[]; hasMore: boolean }

export interface WardrobeBridge {
  state(): Promise<VaultState>;
  categories(): Promise<Category[]>;
  setup(passphrase: string): Promise<VaultState>;
  unlock(passphrase: string): Promise<VaultState>;
  changePassphrase(current: string, replacement: string): Promise<void>;
  list(query: Query): Promise<Page>;
  detail(id: string): Promise<Garment>;
  save(id: string | null, input: GarmentInput, photo: PhotoChange): Promise<Garment>;
  delete(id: string): Promise<void>;
  image(id: string, thumbnail: boolean): Promise<ArrayBuffer>;
  backups(): Promise<string[]>;
  backup(chooseLocation: boolean): Promise<string>;
  restore(passphrase: string, name: string | null): Promise<VaultState>;
  cancel(): Promise<void>;
}

export const wardrobe: WardrobeBridge | null = isTauri() ? {
  state: () => invoke('wardrobe_state'),
  categories: () => invoke('wardrobe_categories'),
  setup: (passphrase) => invoke('wardrobe_setup', { passphrase }),
  unlock: (passphrase) => invoke('wardrobe_unlock', { passphrase }),
  changePassphrase: (current, replacement) => invoke('wardrobe_change_passphrase', { current, replacement }),
  list: (query) => invoke('wardrobe_list', { query }),
  detail: (id) => invoke('wardrobe_detail', { id }),
  save: (id, input, photo) => invoke('wardrobe_save', { id, input, photo }),
  delete: (id) => invoke('wardrobe_delete', { id }),
  image: (id, thumbnail) => invoke('wardrobe_image', { id, thumbnail }),
  backups: () => invoke('wardrobe_backups'),
  backup: (chooseLocation) => invoke('wardrobe_create_backup', { chooseLocation }),
  restore: (passphrase, name) => invoke('wardrobe_restore', { passphrase, name }),
  cancel: () => invoke('wardrobe_cancel_backup'),
} : null;

const errors = {
  busy: 'The wardrobe is in use. Close any other copy of FitSense and retry.',
  locked: 'Unlock the wardrobe before making changes.',
  exists: 'A wardrobe or backup already exists at the destination.',
  invalid_input: 'Check the required fields and the length of optional details.',
  invalid_passphrase: 'Use a passphrase with at least 12 characters and no more than 256 bytes.',
  wrong_passphrase: 'The passphrase could not unlock the recovery key. Check it and try again.',
  local_key: 'Windows could not protect or open the local key.',
  encryption_unavailable: 'Secure storage is unavailable. Your wardrobe has been left untouched.',
  database: 'The encrypted database could not complete this operation.',
  integrity: 'FitSense could not verify the encrypted data. Try a different backup if available.',
  io: 'FitSense could not read or write the local files. Check the drive and available space.',
  invalid_image: 'Choose a valid JPEG or PNG, up to 12 MB and 20 megapixels.',
  image_missing: 'A photo is missing from the encrypted wardrobe.',
  not_found: 'The requested garment or backup could not be found.',
  incompatible: 'This data format or schema is incompatible with this version of FitSense.',
  cancelled: 'The operation was cancelled.',
  cleanup: 'FitSense could not finish cleaning up encrypted files.',
};

export function wardrobeError(error: unknown): string {
  return typeof error === 'string' && Object.hasOwn(errors, error)
    ? errors[error as keyof typeof errors]
    : 'FitSense could not complete the wardrobe operation.';
}

export const emptyGarment: GarmentInput = {
  name: '', category: '', primaryColour: '', secondaryColours: [], pattern: '',
  fit: '', sleeve: '', fabric: '', formality: '', seasons: [], notes: '',
};

export function filePhoto(file: File): Promise<PhotoChange> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const dataUrl = String(reader.result);
      const offset = dataUrl.indexOf(',');
      if (offset < 0) reject('invalid_image');
      else resolve({ kind: 'upload', base64: dataUrl.slice(offset + 1) });
    };
    reader.onerror = () => reject('invalid_image');
    reader.readAsDataURL(file);
  });
}
