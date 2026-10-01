<img width="115" height="86.4" alt="pinhole pixel art" src="src/readme.png" />

# pinhole

view and control your mac from a browser on the same wi-fi. run it in your terminal, then connect from your phone or another computer. the connection is encrypted and stays on your local network.

## install

requires macos 12.3+ and node.js 18+. the npm package includes native binaries for apple silicon and intel macs, so rust and xcode are not required.

```sh
npm install -g pinhole
pinhole permissions
```

or install it in a project with `npm install pinhole` and run it with `npx pinhole`.

enable pinhole in system settings → privacy & security → screen recording and accessibility. keep the permissions command running while you do this, then press ctrl-c.

## use

```sh
pinhole
```

on your other device, open the host address printed in the terminal. compare its certificate fingerprint with the terminal before accepting the browser warning, then enter the six-digit code.

tap, drag, scroll, or type to control your mac. keep the terminal open; ctrl-c stops sharing.

for background sharing:

```sh
pinhole start
pinhole status
pinhole stop
```

## feedback

if you try pinhole, please [open an issue](https://github.com/Luthiraa/pinhole/issues) with your macos version, mac model, client device and browser, and what worked or failed. feedback on setup, latency, image quality, controls, and the security model is especially useful.
