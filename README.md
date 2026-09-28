<img width="115" height="86.4" style="margin-bottom: -10px" alt="Pinhole pixel art" src="src/readme.png" />

# Pinhole

View and control your Mac from a browser on the same Wi-Fi. Start sharing from your terminal. No account or relay.

## Setup

Requires macOS 11 or later, Node.js 18+ with npm, Rust, and Xcode Command Line Tools. Build the package locally; it includes Apple Silicon and Intel binaries.

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm pack
npm install -g ./luthiraa-pinhole-0.3.0.tgz
pinhole permissions
pinhole
```

Keep `pinhole permissions` running while you enable **Pinhole** in **Screen Recording** and **Accessibility**. Then press **Ctrl-C** and run `pinhole`. A bundled background app owns these permissions; your terminal needs no recording or control access. On your phone, open the **https://** address printed in the terminal and enter its six-digit code. Tap, drag, scroll, or type to control the Mac. **Ctrl-C** stops sharing.

Use `pinhole host <mac-lan-ip> [port]` to choose an address, or `pinhole --help` for help.

To share in the background, run `pinhole start [mac-lan-ip] [port]`. It prints the address and code and returns your terminal. `pinhole status` shows the connection details; `pinhole stop` stops sharing. This is a normal, visible macOS user service, started manually. It does not start at login or restart after failure. Its private log and service definition are stored in `~/.pinhole`, outside the login startup folders. Screen Recording and Accessibility permissions still apply.

The CLI installs its app at `~/Applications/Pinhole.app`. If Pinhole is missing from either permission list, click **+** and select that app.

## Security

Pinhole connects directly over HTTPS on your local network. Its locally generated certificate causes a browser warning: compare the browser's SHA-256 fingerprint with the one printed in the terminal before continuing. Do not forward the port to the internet. Captures are briefly written to `~/.pinhole`; an interrupted capture may remain there until the next launch. The helper stops when the CLI disconnects, including when the terminal closes.

## Develop

`src` contains the Rust host, browser UI, and background service. `src/cli.js` is the npm CLI launcher.

Check the source with `cargo check --locked`, `cargo fmt --check`, `node --check src/app.js`, `node --check src/service.js`, and `node --check src/cli.js`.

Ad hoc rebuilds may prompt for permissions again. Set `PINHOLE_SIGN_IDENTITY` to use a stable Apple signing identity.

Copyright © 2026 Luthiraa. Licensed under the [GNU General Public License v3.0](LICENSE).
