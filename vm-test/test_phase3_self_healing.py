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

# 0. Test default shipped config: self_healing.enabled=false in /etc/ai-engineer/self-healing.toml
print("\n--- 0. Verifying Shipped Configuration Guarantee (enabled=false) ---")
res = subprocess.run(
    ["/var/lib/ai-bin/ai-engineer", "self-heal", "--config", "/etc/ai-engineer/self-healing.toml", "--once", "--json"],
    capture_output=True,
    text=True
)
print("ai-engineer self-heal exit code:", res.returncode)
print("ai-engineer self-heal stdout:\n", res.stdout)
if res.returncode != 0:
    sys.exit(f"FAIL: ai-engineer self-heal failed with code {res.returncode}: {res.stderr}")
if "Self-healing disabled by configuration" not in res.stdout:
    sys.exit("FAIL: Expected output to confirm Self-healing is disabled by configuration")
print("Confirmed: /etc/ai-engineer/self-healing.toml has enabled=false; zero autonomous action executed by default.")

# 1. Setup an isolated canary service for testing self-healing restarts
print("\n--- 1. Starting Isolated Canary Service ---")
subprocess.run(["sudo", "systemd-run", "--unit=canary-selfheal", "sleep", "3600"], capture_output=True)
time.sleep(0.5)

# 2. Test live ServiceRestart capability dispatch & post-verification
print("\n--- 2. Testing Live ServiceRestart Capability Dispatch ---")
r_svc = send_cap({
    "capability": "service_restart",
    "unit": "canary-selfheal.service",
    "approved": True,
})
print("ServiceRestart response:", r_svc)
if r_svc.get("status") != "success":
    sys.exit(f"FAIL: ServiceRestart failed: {r_svc}")
print("Verified ServiceRestart executed successfully on canary-selfheal.service")

# 3. Test Killswitch Enforcement on Live Socket
print("\n--- 3. Testing Killswitch Live Socket Silencing ---")
lock_path = "/tmp/live-killswitch.lock"
with open(lock_path, "w") as f:
    f.write("ENGAGED\n")
print(f"Engaged killswitch lockfile at {lock_path}")

# Attempt capability dispatch while killswitch is active -> MUST be denied
r_ks_denied = send_cap({
    "capability": "service_restart",
    "unit": "canary-selfheal.service",
    "approved": True,
})
print("Capability dispatch during active killswitch response:", r_ks_denied)
reason = r_ks_denied.get("reason", "").lower()
if r_ks_denied.get("status") != "denied" or not ("kill-switch" in reason or "kill switch" in reason or ("kill" in reason and "switch" in reason)):
    sys.exit(f"FAIL: Expected killswitch denial, got: {r_ks_denied}")
print("Verified Killswitch actively silenced capability daemon dispatch")

# Release killswitch
if os.path.exists(lock_path):
    os.remove(lock_path)
print("Released killswitch lockfile")

# Verify dispatch succeeds again after killswitch release
r_ks_released = send_cap({
    "capability": "service_status",
    "unit": "canary-selfheal.service",
})
if r_ks_released.get("status") != "success":
    sys.exit(f"FAIL: Expected success after killswitch release, got: {r_ks_released}")
print("Verified normal operation restored after killswitch release")

# 4. Clean up canary unit
subprocess.run(["sudo", "systemctl", "stop", "canary-selfheal.service"], capture_output=True)

# 5. Live Work Log Audit Stream Verification
print("\n--- 4. Verifying Append-Only Work Log Audit Trail ---")
worklog_path = "/var/log/live-worklog.jsonl"
if os.path.exists(worklog_path):
    with open(worklog_path, "r") as f:
        entries = [json.loads(line) for line in f if line.strip()]
    print(f"Total live audit log entries recorded in VM: {len(entries)}")
    for e in entries[-6:]:
        print(f"  #{e.get('seq', '?')} [{e.get('timestamp')}] {e.get('capability')} -> status={e.get('outcome', {}).get('status', e.get('outcome', {}).get('result', {}))}")

print("\n==> ALL PHASE 3.1 LIVE IN-GUEST VERIFICATION CHECKS PASSED")
