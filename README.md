<img width="115" height="86.4" alt="pinhole pixel art" src="src/readme.png" />

# pinhole

view and control your mac from a browser on the same wi-fi. run it in your terminal, then connect from your phone or another computer. the connection is encrypted and stays on your local network.

## how to use

you need macos 12.3+, node.js 18+, rust, and the xcode command line tools.

build and install:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm pack
npm install -g ./luthiraa-pinhole-*.tgz
pinhole permissions
```

enable pinhole in system settings → privacy & security → screen recording and accessibility. keep the command running while you do this, then press ctrl-c.

start sharing:

```sh
pinhole
```

on your other device, open the host address printed in the terminal. compare its certificate fingerprint with the terminal before accepting the browser warning, then enter the six-digit code.

tap, drag, scroll, or type to control your mac. keep the terminal open; ctrl-c stops sharing.

for background sharing, use `pinhole start`. `pinhole status` shows the current address and code; `pinhole stop` stops it.
