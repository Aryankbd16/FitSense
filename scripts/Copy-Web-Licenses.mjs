import { copyFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';

const output = 'src-tauri/runtime/licenses';
const licenses = [
  ['react', 'LICENSE', 'react.txt'],
  ['react-dom', 'LICENSE', 'react-dom.txt'],
  ['lucide-react', 'LICENSE', 'lucide-react.txt'],
  ['@tauri-apps/api', 'LICENSE-APACHE-2.0', 'tauri-api-apache.txt'],
  ['@tauri-apps/api', 'LICENSE-MIT', 'tauri-api-mit.txt'],
  ['@fontsource-variable/dm-sans', 'LICENSE', 'dm-sans.txt'],
  ['@fontsource-variable/manrope', 'LICENSE', 'manrope.txt'],
];

await mkdir(output, { recursive: true });
await Promise.all(licenses.map(([name, license, destination]) =>
  copyFile(join('node_modules', name, license), join(output, destination))));
await Promise.all(['sqlcipher.txt', 'openssl.txt'].map((license) =>
  copyFile(join('licenses', license), join(output, license))));
