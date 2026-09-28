#!/usr/bin/env node
const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const net = require('node:net');
const os = require('node:os');
const path = require('node:path');

const bundledApp = path.resolve(__dirname, '../Pinhole.app');
const executable = 'Contents/MacOS/pinhole';
const bundledBinary = path.join(bundledApp, executable);
const args = process.argv.slice(2);
if (['start', 'status', 'stop'].includes(args[0])) {
  require('./service')(args).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
  return;
}
if (args.length === 1 && ['--help', '-h', '--version', '-V'].includes(args[0])) {
  const result = spawnSync(bundledBinary, args, { stdio: 'inherit' });
  if (result.error) console.error(result.error.message);
  process.exit(result.status ?? 1);
}

const app = path.join(os.homedir(), 'Applications/Pinhole.app');
const installedBinary = path.join(app, executable);
if (fs.existsSync(app) && fs.lstatSync(app).isSymbolicLink()) {
  console.error(`Remove the Pinhole.app symlink at ${app} before continuing.`);
  process.exit(1);
}
if (!fs.existsSync(installedBinary) ||
    !fs.readFileSync(bundledBinary).equals(fs.readFileSync(installedBinary))) {
  fs.mkdirSync(path.dirname(app), { recursive: true });
  fs.cpSync(bundledApp, app, { recursive: true });
}
if (args.length === 1 && args[0] === 'permissions') {
  console.log(`If Pinhole is missing from Settings, use + to add: ${app}`);
}

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'pinhole-'));
fs.chmodSync(dir, 0o700);
fs.writeFileSync(path.join(dir, 'start.json'), JSON.stringify({
  args,
  state_dir: process.env.PINHOLE_STATE_DIR || null,
  background: process.env.PINHOLE_BACKGROUND === '1',
}), { mode: 0o600 });
let connection;
let stopped = false;
let exited = false;
let exitCode = 1;
let timer;

function finish() {
  if (!exited || (connection && !connection.destroyed)) return;
  clearTimeout(timer);
  server.close();
  fs.rmSync(dir, { recursive: true, force: true });
  process.exitCode = exitCode;
}

for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(signal, () => {
    stopped = true;
    connection?.end();
  });
}

const server = net.createServer(socket => {
  if (connection) return socket.destroy();
  connection = socket;
  clearTimeout(timer);
  server.close();
  socket.pipe(process.stdout, { end: false });
  socket.on('error', () => {});
  socket.on('close', finish);
  if (stopped) socket.end();
});
server.on('error', error => {
  console.error(error.message);
  exited = true;
  finish();
});
server.listen(path.join(dir, 'control.sock'), () => {
  // Launch Services gives the helper its own macOS permission identity.
  const launch = spawn('/usr/bin/open', ['-n', '-W', '-g', app, '--args', '--session', dir], {
    detached: true,
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  launch.stderr.pipe(process.stderr, { end: false });
  launch.on('error', error => console.error(error.message));
  launch.on('close', code => {
    try {
      exitCode = Number(fs.readFileSync(path.join(dir, 'exit'), 'utf8'));
    } catch {
      exitCode = code || 1;
      console.error('Pinhole helper could not start. Reinstall the npm package and try again.');
    }
    exited = true;
    finish();
  });
  timer = setTimeout(() => {
    console.error('Pinhole helper startup timed out.');
    launch.kill('SIGTERM');
  }, 30000);
});
