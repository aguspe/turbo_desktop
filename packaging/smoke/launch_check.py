#!/usr/bin/env python3
"""Launch a packaged bundle and hold it to its contract.

The claim is not that the app was built. It is that the artifact starts,
announces where it is listening, answers a request, and then exits when its
parent closes stdin rather than leaving a server behind.
"""
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request

launcher = sys.argv[1] if len(sys.argv) > 1 else "dist/Smoke.app/Contents/MacOS/launch"

proc = subprocess.Popen(
    [launcher],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)

line = ""
for _ in range(300):
    line = proc.stdout.readline()
    if line.strip() or proc.poll() is not None:
        break

if not line.strip():
    print("FAIL  no handshake on stdout")
    print(proc.stderr.read()[:2000])
    sys.exit(1)

try:
    handshake = json.loads(line)
except json.JSONDecodeError:
    print(f"FAIL  first stdout line was not a handshake: {line.strip()[:120]!r}")
    print("      stdout must carry only the handshake; Puma's own output goes to stderr")
    sys.exit(1)

print(f"OK    handshake {handshake['url']} pid={handshake['pid']}")

try:
    with urllib.request.urlopen(handshake["url"] + "/up", timeout=20) as response:
        if response.status != 200:
            print(f"FAIL  GET /up returned {response.status}")
            sys.exit(1)
        print("OK    GET /up 200")
except urllib.error.HTTPError as error:
    print(f"FAIL  GET /up returned {error.code}")
    sys.exit(1)
except Exception as error:  # noqa: BLE001 - any failure here is a failure
    print(f"FAIL  GET /up raised {type(error).__name__}: {error}")
    sys.exit(1)

# Closing stdin is the only exit signal that survives the parent being
# force-quit, since no shell code runs then.
proc.stdin.close()
for tick in range(60):
    if proc.poll() is not None:
        print(f"OK    exited {round(tick * 0.2, 1)}s after stdin closed, code {proc.returncode}")
        break
    time.sleep(0.2)
else:
    print("FAIL  still running after stdin closed; this would orphan a server")
    proc.kill()
    sys.exit(1)
