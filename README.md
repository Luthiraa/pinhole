# Pinhole

Control your Mac from a browser on the same local network. Pinhole has no account, relay, analytics, or certificate transfer. The Mac app shows a six-digit session code. The connection uses HTTPS with a certificate generated locally on the Mac.

## Start sharing

1. Run `./build-app.sh` on the Mac, then open `dist/Pinhole.app`.
2. In its window, choose **Allow Screen Recording**. Choose **Allow Remote Control**, then turn on Pinhole in **System Settings > Privacy & Security > Accessibility**. If it is missing, use **+** there to add `dist/Pinhole.app`. Restart Pinhole if macOS asks.
3. On a device using the same Wi-Fi, open the **https://** address shown in the Mac window. Enter the six-digit code. Close the Mac window or choose **Stop Sharing** to end the session.

The browser may show a certificate warning because Pinhole creates its own certificate. The Mac window shows the certificate's SHA-256 fingerprint. If your browser shows the same fingerprint in its certificate details, compare them before continuing. No certificate file needs to be installed on the other device.

The browser displays the Mac's main screen. Tap or click, drag, scroll, and type to control it. A new code is generated on each launch. Five wrong codes pause attempts for one minute.

## Privacy and security

- Screen images and input travel directly over the local network through TLS. No Pinhole service or third-party server receives them.
- Pinhole listens only on a private IPv4 address, and protects screen and control endpoints with the session code. The browser does not store the code after the page closes.
- The Mac creates its own TLS key in `~/.pinhole/tls.json`, readable only by the current user. A screenshot is briefly written inside that private directory while macOS captures it, then deleted. An unclean shutdown can leave a screenshot there until the next launch; you can remove `~/.pinhole/capture.png` at any time.
- **Certificate warning limit:** HTTPS encrypts traffic, but a browser cannot automatically authenticate a self-signed certificate. If you continue past the warning without checking the certificate fingerprint, an active attacker on the local network could impersonate the Mac and see the code or screen. A publicly trusted certificate or a separately installed phone app is needed to remove this first-connection trust step. This build should not be exposed to the internet.

## Build and verify

Requires macOS, Rust, and Xcode Command Line Tools. Run `cargo test --locked`, `cargo fmt --check`, `node --check src/app.js`, and `./build-app.sh`. The build uses an ad hoc signature by default, so macOS may request permissions again after a rebuild. Release maintainers can set `PINHOLE_SIGN_IDENTITY` to a stable Apple signing identity. Distributing a signed, notarized app requires an Apple Developer account.

The app binds to the first private IPv4 address it finds. If that is not your Wi-Fi address, run `./target/release/pinhole host <mac-lan-ip> [port]` from Terminal. The default port is `48731`. Keep the Mac awake and unlocked, and permit the connection in the Mac firewall if prompted.

The source is MIT licensed. The repository can be published when its owner is ready.
