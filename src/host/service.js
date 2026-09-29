const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { setTimeout: delay } = require('node:timers/promises');

module.exports = async function service([command, ...args]) {
  if (process.platform !== 'darwin') throw new Error('The host requires macOS.');
  if ((command === 'start' && args.length > 1) || (command !== 'start' && args.length)) {
    throw new Error('Usage: pinhole start [ip], pinhole status, or pinhole stop');
  }
  const root = path.resolve(process.env.PINHOLE_STATE_DIR || path.join(os.homedir(), '.pinhole'));
  const label = 'com.luthiraa.pinhole.host';
  const domain = `gui/${process.getuid()}`;
  const target = `${domain}/${label}`;
  const log = path.join(root, 'host.log');
  const plist = path.join(root, 'host.plist');
  const launchctl = (...parameters) => {
    const result = spawnSync('/bin/launchctl', parameters, { encoding: 'utf8', timeout: 15000 });
    if (result.error) throw result.error;
    return result;
  };
  const check = result => {
    if (result.status !== 0) throw new Error(result.stderr.trim() || 'launchctl failed');
  };
  const inspect = () => launchctl('print', target);
  const running = result => result.status === 0 && /\bstate = running\b/.test(result.stdout);
  const output = () => fs.existsSync(log) ? fs.readFileSync(log, 'utf8') : '';
  const current = inspect();

  if (command === 'status') {
    console.log(running(current) ? `Pinhole is running (${target}).\n${output()}` : 'Pinhole is stopped.');
    return;
  }
  if (command === 'stop') {
    if (current.status === 0) check(launchctl('bootout', target));
    console.log('Pinhole stopped.');
    return;
  }
  if (running(current)) {
    console.log(`Pinhole is already running. Use pinhole stop before changing its address.\n${output()}`);
    return;
  }
  if (current.status === 0) check(launchctl('bootout', target));
  if (fs.existsSync(root) && fs.lstatSync(root).isSymbolicLink()) {
    throw new Error('Pinhole state directory must not be a symlink.');
  }
  fs.mkdirSync(root, { recursive: true, mode: 0o700 });
  fs.chmodSync(root, 0o700);
  for (const file of [log, plist]) {
    if (fs.existsSync(file) && fs.lstatSync(file).isSymbolicLink()) {
      throw new Error(`${file} must not be a symlink.`);
    }
  }
  const definition = {
    Label: label,
    ProgramArguments: [process.execPath, fs.realpathSync(process.argv[1]), ...(args.length ? ['host', ...args] : [])],
    EnvironmentVariables: { HOME: os.homedir(), PINHOLE_STATE_DIR: root, PINHOLE_BACKGROUND: '1' },
    RunAtLoad: true,
    KeepAlive: false,
    StandardOutPath: log,
    StandardErrorPath: log,
    Umask: 0o077,
  };
  const converted = spawnSync('/usr/bin/plutil', ['-convert', 'xml1', '-o', '-', '-'], {
    input: JSON.stringify(definition), encoding: 'utf8',
  });
  if (converted.error) throw converted.error;
  check(converted);
  fs.writeFileSync(plist, converted.stdout, { mode: 0o600 });
  fs.writeFileSync(log, '', { mode: 0o600 });
  fs.chmodSync(plist, 0o600);
  fs.chmodSync(log, 0o600);
  // Kept outside LaunchAgents: bootstrap starts it only for this login session.
  check(launchctl('bootstrap', domain, plist));
  try {
    for (let attempt = 0; attempt < 150; attempt++) {
      await delay(100);
      const contents = output();
      if (contents.includes('Background host ready.') && running(inspect())) {
        console.log(contents.trim());
        return;
      }
      if (/Error:|could not start|timed out/.test(contents)) throw new Error(contents.trim());
    }
    throw new Error(`Pinhole did not start within 15 seconds.\n${output()}`);
  } catch (error) {
    launchctl('bootout', target);
    throw error;
  }
};
