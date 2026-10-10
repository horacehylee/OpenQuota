// Builds the portable Windows exe and publishes it as a timestamped GitHub
// release. Tag convention: portable-<version>-<yyyyMMddHHmm> in the local
// clock (HKT for this machine), so repeated builds of the same upstream
// version stay distinguishable. The uploaded asset keeps the plain
// `openquota.exe` name.

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';

const dryRun = process.argv.includes('--dry-run');

function run(command, args, useShell = false) {
  const result = spawnSync(command, args, { stdio: 'inherit', shell: useShell });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function pad(value) {
  return String(value).padStart(2, '0');
}

// The local machine runs in HKT, so local time is the release stamp.
function hktStamp() {
  const now = new Date();
  return (
    `${now.getFullYear()}` +
    `${pad(now.getMonth() + 1)}` +
    `${pad(now.getDate())}` +
    `${pad(now.getHours())}` +
    `${pad(now.getMinutes())}`
  );
}

function findGh() {
  const candidates = [
    'gh',
    'C:\\Program Files\\GitHub CLI\\gh.exe',
    'C:\\ProgramData\\chocolatey\\bin\\gh.exe',
  ];
  for (const candidate of candidates) {
    if (candidate === 'gh' || existsSync(candidate)) {
      const probe = spawnSync(candidate, ['--version'], { stdio: 'ignore' });
      if (probe.status === 0) {
        return candidate;
      }
    }
  }
  console.error(
    'GitHub CLI (gh) not found. Install it with `choco install gh` and run `gh auth login`.',
  );
  process.exit(1);
}

const version = JSON.parse(readFileSync('package.json', 'utf8')).version;
const stamp = hktStamp();
const tag = `portable-${version}-${stamp}`;
const exePath = 'src-tauri/target/release/openquota.exe';

console.log(`Building portable exe for ${tag}...`);
// corepack/pnpm are .cmd shims on Windows, which spawnSync cannot exec
// directly; the build arguments contain no spaces, so the shell is safe here.
run('corepack', ['pnpm', 'build:portable'], process.platform === 'win32');

if (!existsSync(exePath)) {
  console.error(`Build finished but ${exePath} is missing.`);
  process.exit(1);
}

if (dryRun) {
  console.log(`Dry run: would tag ${tag} and publish ${exePath} to GitHub releases.`);
  process.exit(0);
}

const existingTags = spawnSync('git', ['tag', '--list', tag], { encoding: 'utf8' });
if (existingTags.stdout.trim() !== '') {
  console.error(`Tag ${tag} already exists. Wait a minute and rerun.`);
  process.exit(1);
}

run('git', ['tag', tag]);
run('git', ['push', 'origin', tag]);

const gh = findGh();
run(gh, [
  'release',
  'create',
  tag,
  exePath,
  '--title',
  `OpenQuota ${version} portable (${stamp} HKT)`,
  '--notes',
  `Standalone Windows build, built ${stamp} HKT. Requires WebView2 (preinstalled on ` +
    'Windows 10/11). Settings stay in %APPDATA%\\Roaming\\io.github.deviffyy.openquota.',
]);
