import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { App } from '../App';
import { emptyGarment } from './api';
import type { Garment, WardrobeBridge } from './api';
import { Wardrobe } from './Wardrobe';

afterEach(cleanup);

const open = { state: 'open', cleanupPending: false } as const;
const shirt: Garment = {
  id: 'shirt', added: 1700000000000, changed: 1700000000000, photo: false,
  details: { ...emptyGarment, name: 'Oxford shirt', category: 'casual-shirts', primaryColour: 'Blue' },
};

function bridge(initial: Garment[] = []): WardrobeBridge {
  let items = [...initial];
  return {
    state: vi.fn(async () => open),
    categories: vi.fn(async () => [
      { id: 'casual-shirts', group: 'Upper wear', label: 'Casual shirts' },
      { id: 'jeans', group: 'Lower wear', label: 'Jeans' },
    ]),
    setup: vi.fn(async () => open),
    unlock: vi.fn(async () => open),
    changePassphrase: vi.fn(async () => {}),
    list: vi.fn(async (query) => ({
      garments: items.filter((item) => (!query.category || query.category === item.details.category)
        && item.details.name.toLowerCase().includes(query.search.toLowerCase())),
      hasMore: false,
    })),
    detail: vi.fn(async (id) => items.find((item) => item.id === id) ?? shirt),
    save: vi.fn(async (id, details) => {
      const item = { ...shirt, id: id ?? 'inserted', details };
      items = [...items.filter((existing) => existing.id !== item.id), item];
      return item;
    }),
    delete: vi.fn(async (id) => { items = items.filter((existing) => existing.id !== id); }),
    image: vi.fn(async () => new ArrayBuffer(0)),
    backups: vi.fn(async () => ['example.fitbackup']),
    backup: vi.fn(async () => 'example.fitbackup'),
    restore: vi.fn(async () => open),
    cancel: vi.fn(async () => {}),
  };
}

describe('digital wardrobe', () => {
  it('navigates from the existing shell and renders loading and empty states', async () => {
    render(<App bridge={null} wardrobeBridge={bridge()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Wardrobe' }));
    expect(screen.getByRole('heading', { name: 'Wardrobe' })).toBeDefined();
    expect(screen.getByText('Checking secure storage…')).toBeDefined();
    expect(await screen.findByText('Your wardrobe is empty')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Diagnostics' })).toBeDefined();
  });

  it('validates its form, reviews and creates a garment, edits it and confirms deletion', async () => {
    const client = bridge();
    render(<Wardrobe bridge={client} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Add garment' }));
    const field = screen.getByLabelText('Garment name') as HTMLInputElement;
    expect(field.required).toBe(true);
    fireEvent.change(field, { target: { value: '  ' } });
    fireEvent.change(screen.getByLabelText('Category and subcategory'), { target: { value: 'casual-shirts' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review garment' }));
    expect(screen.getByRole('alert').textContent).toContain('Enter a garment name');
    expect(client.save).not.toHaveBeenCalled();
    fireEvent.change(field, { target: { value: 'New shirt' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review garment' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save garment' }));
    await screen.findByRole('heading', { name: 'New shirt' });
    fireEvent.click(screen.getByRole('button', { name: 'Edit garment' }));
    fireEvent.change(screen.getByLabelText('Garment name'), { target: { value: 'Edited shirt' } });
    fireEvent.click(screen.getByRole('button', { name: 'Review garment' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save garment' }));
    await screen.findByRole('heading', { name: 'Edited shirt' });
    fireEvent.click(screen.getByRole('button', { name: 'Delete garment' }));
    expect(client.delete).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Keep garment' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete garment' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm deletion' }));
    await screen.findByText('Your wardrobe is empty');
    expect(client.delete).toHaveBeenCalledWith('inserted');
  });

  it('sends search and category filters and displays unmatched results', async () => {
    const client = bridge([shirt]);
    render(<Wardrobe bridge={client} />);
    await screen.findByText('Oxford shirt');
    fireEvent.change(screen.getByLabelText('Search garments'), { target: { value: 'wrong' } });
    await screen.findByText('No matching garments');
    fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    await screen.findByText('Oxford shirt');
    fireEvent.change(screen.getByLabelText('Category', { exact: true }), { target: { value: 'jeans' } });
    await screen.findByText('No matching garments');
    expect(client.list).toHaveBeenLastCalledWith(expect.objectContaining({ category: 'jeans' }));
  });

  it('reports a failed secure setup and allows it to be retried', async () => {
    const client = bridge();
    client.state = vi.fn().mockRejectedValueOnce('encryption_unavailable').mockResolvedValue({ state: 'setup' });
    render(<Wardrobe bridge={client} />);
    expect((await screen.findByRole('alert')).textContent).toContain('Secure storage is unavailable');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await screen.findByRole('heading', { name: 'Create your private wardrobe' });
    const phrase = 'a password used only for testing';
    fireEvent.change(screen.getByLabelText('Recovery passphrase', { exact: true }), { target: { value: phrase } });
    fireEvent.change(screen.getByLabelText('Confirm recovery passphrase'), { target: { value: phrase } });
    fireEvent.click(screen.getByRole('button', { name: 'Create secure wardrobe' }));
    await waitFor(() => expect(client.setup).toHaveBeenCalledWith(phrase));
    await screen.findByText('Your wardrobe is empty');
  });
});
