import json
import socket
import sys


def send_cap(cap, is_rb=False):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(180.0)
    s.connect("/tmp/live-cap.sock")
    req = {
        "api_version": 1,
        "id": "live-rb",
        "requester": "vm-smoke",
        "token": "tok-vm",
        "is_rollback": is_rb,
        "capability": cap,
    }
    s.sendall(json.dumps(req).encode() + b"\n")
    resp_raw = s.makefile("r").readline()
    s.close()
    return json.loads(resp_raw)


r1 = send_cap(
    {
        "capability": "file_write",
        "path": "canary.conf",
        "content": "val\n",
        "append": False,
    }
)
print("r1 file_write:", r1, flush=True)
if r1.get("status") != "success":
    sys.exit(f"file_write failed: {r1}")

r2 = send_cap({"capability": "file_delete", "path": "canary.conf"}, is_rb=True)
print("r2 file_delete rb:", r2, flush=True)
if r2.get("status") != "success":
    sys.exit(f"file_delete rb failed: {r2}")

r3 = send_cap(
    {
        "capability": "service_stop",
        "unit": "canary-rollback.service",
        "approved": True,
    },
    is_rb=True,
)
print("r3 service_stop rb:", r3, flush=True)
if r3.get("status") != "success":
    sys.exit(f"service_stop rb failed: {r3}")

r4 = send_cap(
    {"capability": "package_install", "package": "htop", "approved": True},
    is_rb=False,
)
print("r4 package_install:", r4, flush=True)
if r4.get("status") != "success":
    sys.exit(f"package_install failed: {r4}")

r5 = send_cap(
    {"capability": "package_remove", "package": "htop", "approved": True},
    is_rb=True,
)
print("r5 package_remove rb:", r5, flush=True)
if r5.get("status") != "success":
    sys.exit(f"package_remove rb failed: {r5}")
