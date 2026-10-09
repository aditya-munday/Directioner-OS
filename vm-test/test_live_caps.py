import json
import socket
import sys

_req_counter = 0


def send_cap(cap, is_rb=False):
    global _req_counter
    _req_counter += 1
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(180.0)
    s.connect("/tmp/live-cap.sock")
    req = {
        "api_version": 1,
        "id": f"live-cap-{_req_counter}",
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

# --- Protected-unit denial assertions (5d) ---
# A live (--exec) daemon MUST deny restart and stop of sshd.service even when
# approved:true is set. The protection is hardcoded in HandlerConfig and is
# independent of dry-run mode.

r6 = send_cap(
    {"capability": "service_restart", "unit": "sshd.service", "approved": True}
)
print("r6 service_restart sshd.service (live):", r6, flush=True)
if r6.get("status") != "denied":
    sys.exit(
        f"FAIL: live daemon must deny restart of sshd.service, got: {r6}"
    )

r7 = send_cap(
    {"capability": "service_stop", "unit": "sshd.service", "approved": True}
)
print("r7 service_stop sshd.service (live):", r7, flush=True)
if r7.get("status") != "denied":
    sys.exit(
        f"FAIL: live daemon must deny stop of sshd.service, got: {r7}"
    )

# Canary: an unprotected unit with approved:true MUST succeed (confirms daemon
# is live and functional, not generally broken).
r8 = send_cap(
    {
        "capability": "service_restart",
        "unit": "canary-rollback.service",
        "approved": True,
    }
)
print("r8 service_restart canary-rollback.service (live):", r8, flush=True)
if r8.get("status") != "success":
    sys.exit(
        f"FAIL: live daemon failed canary restart (daemon not in live mode?): {r8}"
    )
