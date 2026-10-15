// SPDX-License-Identifier: GPL-3.0-only
// Collect upstream notices from packages referenced by supplied Cargo metadata.
// Usage: node scripts/licenses.cjs metadata.json output-directory
const fs = require('node:fs');
const path = require('node:path');
const [metadataPath, outputRoot] = process.argv.slice(2);
if (!metadataPath || !outputRoot) {
  throw new Error('Usage: node scripts/licenses.cjs metadata.json output-directory');
}
const metadata = JSON.parse(fs.readFileSync(metadataPath, 'utf8').replace(/^\uFEFF/, ''));
const nodes = new Map(metadata.resolve.nodes.map(n => [n.id, n]));
const ids = new Set();
function visit(id) {
  if (ids.has(id)) return;
  ids.add(id);
  for (const dependency of nodes.get(id)?.deps || []) {
    visit(dependency.pkg);
  }
}
for (const id of metadata.workspace_members) visit(id);
const packages = metadata.packages
  .filter(packageInfo => ids.has(packageInfo.id) && packageInfo.source)
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
const notices = path.join(outputRoot, 'licenses');
fs.mkdirSync(notices, {recursive:true});
const rows = [
  '# Dependency notices',
  '',
  'Inventory derived from the supplied Cargo metadata, including build/test dependencies. This is not a list of code embedded in a binary. Upstream notices are copied under licenses/. Add the applicable Rust runtime and toolchain notices when distributing executables.',
  '',
  '| Package | Version | Declared upstream license |',
  '| --- | --- | --- |',
];
let copied = 0;
for(const pkg of packages) {
  const directory = path.dirname(pkg.manifest_path);
  const dest = path.join(notices, `${pkg.name}-${pkg.version}`);
  fs.mkdirSync(dest, {recursive:true});
  for(const entry of fs.readdirSync(directory,{withFileTypes:true})) {
    if(/^(license|licence|copying|notice)/i.test(entry.name)) {
      const src = path.join(directory,entry.name), out = path.join(dest,entry.name);
      if (entry.isFile()) {
        fs.copyFileSync(src, out);
        copied++;
      } else if (entry.isDirectory()) {
        fs.cpSync(src, out, {recursive: true});
        copied++;
      }
    }
  }
  if(pkg.license_file) {
    const source = path.resolve(directory,pkg.license_file);
    if (fs.existsSync(source)) {
      fs.copyFileSync(source, path.join(dest, path.basename(source)));
      copied++;
    }
  }
  const link = pkg.repository || `https://crates.io/crates/${pkg.name}/${pkg.version}`;
  rows.push(`| [${pkg.name}](${link}) | ${pkg.version} | ${(pkg.license || 'see license file').replace(/\|/g,' / ')} |`);
}
fs.writeFileSync(path.join(outputRoot,'THIRD-PARTY.md'),rows.join('\n')+'\n');
console.log(`${packages.length} packages; ${copied} notices copied.`);
