// Copies the release-built openquota-cli binary into a PATH directory (~/.cargo/bin by
// default, which rustup installs on Windows, macOS, and Linux). Run `pnpm build:cli` first.

import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const binary = process.platform === 'win32' ? 'openquota-cli.exe' : 'openquota-cli';
const source = join('src-tauri', 'target', 'release', binary);
const destinationDirectory = process.env.CARGO_BIN || join(homedir(), '.cargo', 'bin');

if (!existsSync(source)) {
  console.error(`${source} is missing. Run \`pnpm build:cli\` first.`);
  process.exit(1);
}

mkdirSync(destinationDirectory, { recursive: true });
const destination = join(destinationDirectory, binary);
copyFileSync(source, destination);
console.log(`Installed ${destination}`);
