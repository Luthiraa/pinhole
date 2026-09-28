"""Installed-package check; --capture verifies real screen access, --kill tests abrupt exit."""

import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import signal
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import time

abrupt = "--kill" in sys.argv[2:]
with tempfile.TemporaryDirectory(prefix="pinhole-cli-", dir="/private/tmp") as temporary:
    root = Path(temporary)
    state = root / "state"
    log_path = root / "host.log"
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    with log_path.open("w") as log:
        process = subprocess.Popen(
            [sys.argv[1], "host", "127.0.0.1", str(port)],
            env=dict(os.environ, PINHOLE_STATE_DIR=str(state), TMPDIR=str(root)),
            stdout=log,
            stderr=log,
        )
        try:
            deadline = time.monotonic() + 15
            while "Keep this terminal open." not in log_path.read_text():
                assert process.poll() is None, "Host exited during startup"
                assert time.monotonic() < deadline, "Host startup timed out"
                time.sleep(0.05)
            output = log_path.read_text()
            apps = json.loads(subprocess.check_output([
                "/usr/bin/osascript", "-l", "JavaScript", "-e", """
                ObjC.import('AppKit');
                var apps = $.NSRunningApplication.runningApplicationsWithBundleIdentifier('com.luthiraa.pinhole');
                var result = [];
                for (var i = 0; i < apps.count; i++) {
                    var app = apps.objectAtIndex(i);
                    result.push({path: ObjC.unwrap(app.bundleURL.path), ready: Boolean(app.finishedLaunching)});
                }
                JSON.stringify(result);
                """
            ], text=True))
            app_path = str(Path.home() / "Applications" / "Pinhole.app")
            assert any(app["path"] == app_path and app["ready"] for app in apps), \
                "Pinhole did not finish native macOS app startup"
            code = re.search(r"^Code  (\d{6})$", output, re.M).group(1)
            identity = json.loads((state / "tls.json").read_text())
            digest = hashlib.sha256(ssl.PEM_cert_to_DER_cert(identity["cert"])).hexdigest()
            assert f"Certificate SHA-256: {digest}" in output
            context = ssl.create_default_context(cadata=identity["cert"])

            def request(method, path, body=None, headers=None):
                connection = http.client.HTTPSConnection(
                    "127.0.0.1", port, context=context, timeout=5
                )
                try:
                    connection.request(method, path, body, headers or {})
                    response = connection.getresponse()
                    return response.status, response.read(), dict(response.getheaders())
                finally:
                    connection.close()

            status, page, headers = request("GET", "/")
            assert status == 200 and b"terminal" in page
            assert headers["cache-control"] == "no-store"
            assert "content-security-policy" in headers
            assert request("POST", "/shot")[0] == 401
            assert request(
                "POST", "/control", "{}", {"Authorization": f"Bearer {code}"}
            )[0] == 400
            if "--capture" in sys.argv[2:]:
                started = time.monotonic()
                status, png, _ = request("POST", "/shot", headers={"Authorization": f"Bearer {code}"})
                assert status == 200, f"Real screen capture failed ({status}): {png.decode(errors='replace')}"
                assert png.startswith(b"\x89PNG\r\n\x1a\n") and len(png) > 24
                width, height = struct.unpack(">II", png[16:24])
                assert width > 0 and height > 0
                print(f"Real screen capture: {width}x{height}, {time.monotonic() - started:.2f}s")
            assert state.stat().st_mode & 0o777 == 0o700
            assert (state / "tls.json").stat().st_mode & 0o777 == 0o600

            # An unfinished TLS handshake must not keep sharing alive after Ctrl-C.
            with socket.create_connection(("127.0.0.1", port), timeout=5):
                time.sleep(0.1)
                (state / "capture.png").write_bytes(b"temporary capture")
                process.send_signal(signal.SIGKILL if abrupt else signal.SIGINT)
                process.wait(timeout=5)
            assert process.returncode == (-signal.SIGKILL if abrupt else 0)
            if not abrupt:
                assert "Stopping sharing" in log_path.read_text()
            deadline = time.monotonic() + 5
            while True:
                with socket.socket() as connection:
                    closed = connection.connect_ex(("127.0.0.1", port)) != 0
                if closed and not (state / "capture.png").exists():
                    break
                assert time.monotonic() < deadline, "Helper did not stop and clean up"
                time.sleep(0.05)
        except Exception:
            print(
                re.sub(r"^Code  .*$", "Code  [redacted]", log_path.read_text(), flags=re.M),
                file=sys.stderr,
            )
            raise
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()

print(f"CLI HTTPS, authentication, private state, and {'abrupt' if abrupt else 'Ctrl-C'} shutdown passed")
