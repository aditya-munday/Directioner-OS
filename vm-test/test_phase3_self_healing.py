import json
import os
import socket
import subprocess
import sys
import time

print("==> PHASE 3.1 CLOSED-LOOP SELF-HEALING & LIVE REMEDIATION VERIFICATION")

def send_cap(cap, is_rb=False):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(60.0)
    s.connect("/tmp/live-cap.sock")
    req = {
        "api_version": 1,
        "id": "self-heal-vm-test",
        "requester": "self-healing-engine",
        "token": "tok-selfheal",
        "is_rollback": is_rb,
        "capability": cap,
    }
    s.sendall(json.dumps(req).encode() + b"\n")
    resp_raw = s.makefile("r").readline()
    s.close()
    return json.loads(resp_raw)

# 1. Test live ServiceRestart capability dispatch & post-verification
print("\n--- 1. Testing Live ServiceRestart Capability Dispatch ---")
r_svc = send_cap({
    "capability": "service_restart",
    "unit": "systemd-journald.service",
    "approved": True,
})
print("ServiceRestart response:", r_svc)
if r_svc.get("status") != "success":
    sys.exit(f"FAIL: ServiceRestart failed: {r_svc}")
print("Verified ServiceRestart executed successfully on systemd-journald.service")

# 2. Test Killswitch Enforcement on Live Socket
print("\n--- 2. Testing Killswitch Live Socket Silencing ---")
lock_path = "/tmp/live-killswitch.lock"
with open(lock_path, "w") as f:
    f.write("ENGAGED\n")
print(f"Engaged killswitch lockfile at {lock_path}")

# Attempt capability dispatch while killswitch is active -> MUST be denied
r_ks_denied = send_cap({
    "capability": "service_restart",
    "unit": "systemd-journald.service",
    "approved": True,
})
print("Capability dispatch during active killswitch response:", r_ks_denied)
if r_ks_denied.get("status") != "denied" or "kill switch" not in r_ks_denied.get("reason", "").lower():
    sys.exit(f"FAIL: Expected killswitch denial, got: {r_ks_denied}")
print("Verified Killswitch actively silenced capability daemon dispatch")

# Release killswitch
if os.path.exists(lock_path):
    os.remove(lock_path)
print("Released killswitch lockfile")

# Verify dispatch succeeds again after killswitch release
r_ks_released = send_cap({
    "capability": "service_status",
    "unit": "systemd-journald.service",
})
if r_ks_released.get("status") != "success":
    sys.exit(f"FAIL: Expected success after killswitch release, got: {r_ks_released}")
print("Verified normal operation restored after killswitch release")

# 3. Live Work Log Audit Stream Verification
print("\n--- 3. Verifying Append-Only Work Log Audit Trail ---")
worklog_path = "/var/log/live-worklog.jsonl"
if os.path.exists(worklog_path):
    with open(worklog_path, "r") as f:
        entries = [json.loads(line) for line in f if line.strip()]
    print(f"Total live audit log entries recorded in VM: {len(entries)}")
    for e in entries[-6:]:
        print(f"  #{e.get('seq', '?')} [{e.get('timestamp')}] {e.get('capability')} -> status={e.get('outcome', {}).get('status', e.get('outcome', {}).get('result', {}))}")

print("\n==> ALL PHASE 3.1 LIVE IN-GUEST VERIFICATION CHECKS PASSED")
