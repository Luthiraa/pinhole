"""Exercise the installed manual background service on loopback only."""
import http.client
import json
import os
from pathlib import Path
import plistlib
import re
import socket
import ssl
import subprocess
import sys
import tempfile
import time

cli = str(Path(sys.argv[1]).resolve())
target = f"gui/{os.getuid()}/com.luthiraa.pinhole.host"
assert subprocess.run(["launchctl", "print", target], capture_output=True).returncode != 0, \
    "Stop the existing background host before running this check"
with tempfile.TemporaryDirectory(prefix="pinhole-service-", dir="/private/tmp") as temporary:
    root = Path(temporary) / "state"
    env = dict(os.environ, PINHOLE_STATE_DIR=str(root))

    def run(*args, success=True):
        result = subprocess.run([cli, *args], env=env, text=True, capture_output=True, timeout=25)
        assert (result.returncode == 0) == success, re.sub(r"Code  \d+", "Code  [redacted]", result.stderr)
        return result.stdout

    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    try:
        assert "stopped" in run("status")
        output = run("start", "127.0.0.1", str(port))
        assert "Background host ready." in output
        code = re.search(r"^Code  (\d{6})$", output, re.M).group(1)
        assert code in run("status")
        assert "already running" in run("start", "127.0.0.1", str(port))
        definition = plistlib.loads((root / "host.plist").read_bytes())
        assert definition["KeepAlive"] is False
        assert definition["EnvironmentVariables"]["PINHOLE_BACKGROUND"] == "1"
        assert root.stat().st_mode & 0o777 == 0o700
        for name in ("host.log", "host.plist", "tls.json"):
            assert (root / name).stat().st_mode & 0o777 == 0o600
        cert = json.loads((root / "tls.json").read_text())["cert"]
        connection = http.client.HTTPSConnection("127.0.0.1", port, context=ssl.create_default_context(cadata=cert), timeout=5)
        for resource in ("/", "/app.js", "/style.css", "/favicon.svg"):
            connection.request("GET", resource)
            response = connection.getresponse()
            assert response.status == 200 and response.read()
        connection.request("POST", "/shot")
        response = connection.getresponse()
        assert response.status == 401
        response.read()
        if "--capture" in sys.argv[2:]:
            connection.request("POST", "/shot", headers={"Authorization": f"Bearer {code}"})
            response = connection.getresponse()
            assert response.status == 200 and response.read().startswith(b"\x89PNG\r\n\x1a\n")
        connection.close()
        (root / "capture.png").write_bytes(b"cleanup check")
        run("stop")
        deadline = time.monotonic() + 5
        while True:
            with socket.socket() as probe:
                closed = probe.connect_ex(("127.0.0.1", port)) != 0
            if closed and not (root / "capture.png").exists():
                break
            assert time.monotonic() < deadline, "Background host did not stop and clean up"
            time.sleep(.1)
        assert "stopped" in run("status")
        run("stop")
        with socket.socket() as occupied:
            occupied.bind(("127.0.0.1", port))
            occupied.listen()
            run("start", "127.0.0.1", str(port), success=False)
        assert "stopped" in run("status")
        assert "Background host ready." in run("start", "127.0.0.1", str(port))
    finally:
        run("stop")
print("Manual service start, status, HTTPS, duplicate start, cleanup, failed start, and restart passed")
