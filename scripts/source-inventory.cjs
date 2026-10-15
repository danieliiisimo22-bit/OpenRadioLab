#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
'use strict';

// Reproducible first-party source inventory. This does not emulate GitHub Linguist.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const root = path.resolve(__dirname, '..');
const ignored = new Set([
    '.git', 'target', 'dist', 'node_modules', 'windows-portable', 'licenses',
    'open-radio-lab-data', 'radio-lab-data', 'private-samples', 'private-qa',
]);
const languages = new Map([
    ['.rs', 'Rust'], ['.slint', 'Slint'], ['.cjs', 'JavaScript'],
    ['.mjs', 'JavaScript'], ['.js', 'JavaScript'], ['.ps1', 'PowerShell'],
    ['.sh', 'Shell'], ['.cmd', 'Batchfile'], ['.bat', 'Batchfile'],
]);

function collect(directory) {
    const files = [];
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        if (ignored.has(entry.name)) continue;
        const absolute = path.join(directory, entry.name);
        if (entry.isDirectory()) files.push(...collect(absolute));
        else if (entry.isFile() && languages.has(path.extname(entry.name))) {
            const content = fs.readFileSync(absolute);
            const text = content.toString('utf8');
            files.push({
                path: path.relative(root, absolute).split(path.sep).join('/'),
                language: languages.get(path.extname(entry.name)),
                bytes: content.length,
                lines: text.split('\n').length - (text.endsWith('\n') ? 1 : 0),
                sha256: crypto.createHash('sha256').update(content).digest('hex'),
            });
        }
    }
    return files;
}

const files = collect(root).sort((a, b) => a.path.localeCompare(b.path, 'en'));
const totalBytes = files.reduce((sum, file) => sum + file.bytes, 0);
const totals = new Map();
for (const file of files) {
    const value = totals.get(file.language) || { language: file.language, files: 0, bytes: 0, lines: 0 };
    value.files += 1;
    value.bytes += file.bytes;
    value.lines += file.lines;
    totals.set(file.language, value);
}
const breakdown = [...totals.values()].sort((a, b) => b.bytes - a.bytes).map(value => ({
    ...value, percentByBytes: Number((value.bytes * 100 / totalBytes).toFixed(2)),
}));
const report = {
    schemaVersion: 1,
    method: 'UTF-8 source bytes by extension; first-party code, tests and scripts; no dependency or generated code',
    excluded: 'documentation, manifests/configuration, Cargo.lock, binaries, dependencies, build outputs and private device data',
    totalBytes, totalFiles: files.length, languages: breakdown, files,
};
const table = [
    '| Language | Files | Bytes | Lines | Share of source bytes |',
    '| --- | ---: | ---: | ---: | ---: |',
    ...breakdown.map(value => `| ${value.language} | ${value.files} | ${value.bytes} | ${value.lines} | ${value.percentByBytes.toFixed(2)}% |`),
].join('\n');
const markdown = `# Source languages\n\n${table}\n\nTotal: ${totalBytes} UTF-8 bytes across ${files.length} source files. Percentages are rounded independently.\n\nCounts include application code, tests and development scripts. They exclude documentation, manifests, workflow configuration, Cargo.lock, dependencies, generated output and device data. These are source-composition percentages, not feature-completion percentages.\n\nRun \`node scripts/source-inventory.cjs\` to regenerate this report. Run with \`--check\` to detect stale reports. The JSON inventory records every input file and its SHA-256.\n\nGitHub uses its own Linguist classifiers and byte counts; its language bar may differ. No source language is relabeled to inflate Rust's share.\n`;
const expected = new Map([
    ['docs/source-inventory.json', JSON.stringify(report, null, 2) + '\n'],
    ['docs/source-languages.md', markdown],
]);
const readme = fs.readFileSync(path.join(root, 'README.md'), 'utf8');
const startMarker = '<!-- source-languages:start -->';
const endMarker = '<!-- source-languages:end -->';
const startIndex = readme.indexOf(startMarker);
const endIndex = readme.indexOf(endMarker);
if (startIndex < 0 || endIndex < startIndex) throw new Error('Missing README language-report markers');
expected.set('README.md', readme.slice(0, startIndex + startMarker.length) + '\n\n' + table + '\n\n' + readme.slice(endIndex));
const check = process.argv.includes('--check');
let stale = false;
for (const [relative, content] of expected) {
    const file = path.join(root, relative);
    if (check) {
        if (!fs.existsSync(file) || fs.readFileSync(file, 'utf8') !== content) {
            process.stderr.write(`Outdated inventory: ${relative}\n`);
            stale = true;
        }
    } else fs.writeFileSync(file, content);
}
if (stale) process.exitCode = 1;
else process.stdout.write((check ? 'Inventory verified.\n' : 'Inventory generated.\n') + table + '\n');

