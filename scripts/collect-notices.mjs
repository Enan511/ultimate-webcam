import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

// Run after dependency installation. Output contains public package metadata and
// original notices, never machine-specific paths from cargo metadata.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root, 'third-party-licenses');
fs.mkdirSync(output, { recursive: true });
const inventory = [];
const isNotice = name => /^(licen[cs]e|copying|notice|copyright)([._-]|$)/i.test(name);
function collect(ecosystem, name, version, license, directory) {
  const destination = path.join(output, ecosystem, `${name.replaceAll('/', '__')}-${version}`);
  const files = [];
  function visit(current, depth, relative = '') {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const child = path.join(current, entry.name);
      const local = path.join(relative, entry.name);
      if (entry.isFile() && isNotice(entry.name)) {
        fs.mkdirSync(path.dirname(path.join(destination, local)), { recursive: true });
        fs.copyFileSync(child, path.join(destination, local));
        files.push(local.replaceAll('\\', '/'));
      } else if (entry.isDirectory() && depth > 0 && /^(upstream|licenses?|notices?)$/i.test(entry.name)) {
        visit(child, depth - 1, local);
      }
    }
  }
  visit(directory, 3);
  inventory.push({ ecosystem, name, version, license, notices: files });
}

const cargo = spawnSync('cargo', [
  'metadata', '--locked', '--offline', '--format-version', '1',
  '--filter-platform', 'x86_64-pc-windows-gnu',
  '--manifest-path', path.join(root, 'pc-webcam-studio/src-tauri/Cargo.toml'),
], { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
if (cargo.status !== 0) throw new Error(cargo.stderr || cargo.error?.message || 'cargo metadata failed');
for (const pkg of JSON.parse(cargo.stdout).packages) {
  if (pkg.source) collect('rust', pkg.name, pkg.version, pkg.license, path.dirname(pkg.manifest_path));
}

const frontend = path.join(root, 'pc-webcam-studio');
const lock = JSON.parse(fs.readFileSync(path.join(frontend, 'package-lock.json'), 'utf8'));
for (const [relative, pkg] of Object.entries(lock.packages)) {
  if (!relative || pkg.dev) continue;
  const directory = path.join(frontend, relative);
  if (!fs.existsSync(directory)) continue;
  const manifest = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
  collect('npm', manifest.name, manifest.version, manifest.license ?? pkg.license, directory);
}
inventory.sort((a, b) => `${a.ecosystem}/${a.name}`.localeCompare(`${b.ecosystem}/${b.name}`));
fs.writeFileSync(path.join(output, 'inventory.json'), `${JSON.stringify(inventory, null, 2)}\n`);
fs.writeFileSync(path.join(output, 'README.txt'),
  'Original notices collected from installed locked Rust and frontend packages.\n' +
  'The inventory includes build dependencies and may include alternate platform packages.\n' +
  'A package with no copied notice still retains its declared upstream license.\n' +
  'ADB, UnityCapture, AndroidX, and Kotlin notices are supplied separately.\n');
console.log(`Collected ${inventory.length} package inventories and ${inventory.reduce((n, p) => n + p.notices.length, 0)} notice files.`);
