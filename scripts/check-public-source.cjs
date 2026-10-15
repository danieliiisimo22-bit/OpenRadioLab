#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
'use strict';

// Source-tree guard, not a general secret scanner. Device evidence stays local.
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const skipped = new Set(['.git', 'target', 'dist', 'node_modules']);
const privateDirectories = new Set([
    'windows-portable', 'open-radio-lab-data', 'radio-lab-data',
    'private-samples', 'private-qa', 'backups', 'recovery', 'diagnostics',
]);
const privateExtensions = /\.(dat|bin|log|exe|dll|pdf|zip|decoded|plain|bak)$/i;
const textExtensions = /\.(rs|slint|md|toml|cjs|js|ps1|sh|yml|json|gitignore|gitattributes|editorconfig)$/;
const personalPath = /(?:[A-Z]:[\\/]Users[\\/]|\/(?:Users|home)\/)[^\\/\s]+/i;
const accessToken = /(?:ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{70,})/;
const problems = [];

function inspect(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
        if (skipped.has(entry.name)) continue;
        const absolute = path.join(directory, entry.name);
        const relative = path.relative(root, absolute);
        if (entry.isDirectory()) {
            if (privateDirectories.has(entry.name)) problems.push(`Private runtime directory: ${relative}`);
            else inspect(absolute);
        } else if (entry.isFile()) {
            if (privateExtensions.test(entry.name)) problems.push(`Private or release-only artifact: ${relative}`);
            if (textExtensions.test(entry.name)) {
                const text = fs.readFileSync(absolute, 'utf8');
                if (personalPath.test(text)) problems.push(`Personal absolute path: ${relative}`);
                if (accessToken.test(text)) problems.push(`Possible access token: ${relative}`);
            }
        } else problems.push(`Unsupported filesystem entry: ${relative}`);
    }
}
inspect(root);
if (problems.length) {
    process.stderr.write(problems.join('\n') + '\n');
    process.exitCode = 1;
} else process.stdout.write('Public source guard passed; no device files, release binaries or personal paths found.\n');

