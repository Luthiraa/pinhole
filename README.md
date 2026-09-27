<p align="center">
  <img src="assets/pinhole-readme.png" alt="A Mac screen sending pixels through a pinhole to a phone" width="560">
</p>

# Pinhole

View and control your Mac from a phone browser on the same Wi-Fi. No account or relay.

## Setup

Requires macOS, [Rust](https://rustup.rs), and Xcode Command Line Tools (`xcode-select --install`).

```sh
git clone https://github.com/Luthiraa/pinhole.git
cd pinhole
./build-app.sh
open dist/Pinhole.app
```

In the Mac window, grant **Screen Recording** and **Remote Control** (Accessibility). Reopen Pinhole if macOS asks. On your phone, open the **https://** address shown in that window and enter its six-digit code. Tap, drag, scroll, or type to control the Mac. Close the Mac window to stop sharing.

## Security

Pinhole connects directly over HTTPS on your local network. Its locally generated certificate causes a browser warning: compare the browser's SHA-256 fingerprint with the one in the Mac window before continuing. Do not forward the port to the internet. Captures are briefly written to `~/.pinhole`; an interrupted capture may remain there until the next launch.

## Develop

Run `cargo test --locked`, `cargo fmt --check`, `node --check src/app.js`, and `./build-app.sh`.

Ad hoc rebuilds may prompt for permissions again. Set `PINHOLE_SIGN_IDENTITY` to use a stable Apple signing identity.

[MIT license](LICENSE).
