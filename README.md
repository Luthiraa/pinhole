<img width="115" height="86.4" style="margin-bottom: -10px" alt="Pinhole pixel art" src="assets/pinhole-readme.png" />

# Pinhole

View and control your Mac from a browser on the same Wi-Fi. Start sharing from your terminal. No account or relay.

## Setup

Requires macOS 11 or later and npm. Download `luthiraa-pinhole-0.2.0.tgz` from the **pinhole-npm** artifact in the [latest successful build](https://github.com/Luthiraa/pinhole/actions/workflows/ci.yml). The package includes Apple Silicon and Intel binaries.

```sh
npm install -g ./luthiraa-pinhole-0.2.0.tgz
pinhole permissions
pinhole
```

Enable your terminal (or Pinhole, if listed) in **Screen Recording** and **Accessibility** when macOS asks. Restart the terminal if requested. On your phone, open the **https://** address printed in the terminal and enter its six-digit code. Tap, drag, scroll, or type to control the Mac. **Ctrl-C** stops sharing.

Use `pinhole host <mac-lan-ip> [port]` to choose an address, or `pinhole --help` for help.

## Security

Pinhole connects directly over HTTPS on your local network. Its locally generated certificate causes a browser warning: compare the browser's SHA-256 fingerprint with the one printed in the terminal before continuing. Do not forward the port to the internet. Captures are briefly written to `~/.pinhole`; an interrupted capture may remain there until the next launch.

## Develop

Requires Rust and Xcode Command Line Tools. Run `cargo test --locked`, `cargo fmt --check`, and `node --check src/app.js`. Check an installed package with `python3 tests/cli.py "$(command -v pinhole)"`.

To build the npm package, run `rustup target add aarch64-apple-darwin x86_64-apple-darwin`, then `npm pack`. To install from source, run `cargo install --locked --path .`.

Ad hoc rebuilds may prompt for permissions again. Set `PINHOLE_SIGN_IDENTITY` to use a stable Apple signing identity.

Copyright © 2026 Luthiraa. Licensed under the [GNU General Public License v3.0](LICENSE).
