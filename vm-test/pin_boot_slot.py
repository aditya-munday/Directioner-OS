import json
import socket
import sys

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(180.0)
s.connect("/tmp/live-cap.sock")
req = {
    "api_version": 1,
    "id": "pin-good-slot",
    "requester": "vm-smoke",
    "token": "tok-vm",
    "is_rollback": False,
    "capability": {"capability": "boot_slot_pin", "slot": 0, "approved": True},
}
s.sendall(json.dumps(req).encode() + b"\n")
resp_raw = s.makefile("r").readline()
s.close()
resp = json.loads(resp_raw)
print("boot_slot_pin response:", resp)
if resp.get("status") != "success":
    sys.exit(f"boot_slot_pin failed: {resp}")
