import json
import socket
import subprocess
import sys
import time


def send_cap(cap, is_rb=False):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(60.0)
    s.connect("/tmp/live-cap.sock")
    req = {
        "api_version": 1,
        "id": "tier2-test",
        "requester": "remediation-agent",
        "token": "tok-tier2",
        "is_rollback": is_rb,
        "capability": cap,
    }
    s.sendall(json.dumps(req).encode() + b"\n")
    resp_raw = s.makefile("r").readline()
    s.close()
    return json.loads(resp_raw)


print("==> 1. Testing KernelParamSet Forward & Compensating Rollback")
# 1.1 Read baseline sysctl
baseline = subprocess.check_output(["sysctl", "-n", "net.ipv4.ip_forward"]).decode().strip()
print(f"Current net.ipv4.ip_forward baseline: {baseline}")
target = "1" if baseline == "0" else "0"

# 1.2 Forward KernelParamSet
r1 = send_cap({
    "capability": "kernel_param_set",
    "key": "net.ipv4.ip_forward",
    "value": target,
    "persistent": False,
    "approved": True,
})
print("KernelParamSet forward response:", r1)
if r1.get("status") != "success":
    sys.exit(f"FAIL: KernelParamSet forward failed: {r1}")

new_val = subprocess.check_output(["sysctl", "-n", "net.ipv4.ip_forward"]).decode().strip()
if new_val != target:
    sys.exit(f"FAIL: net.ipv4.ip_forward expected {target}, got {new_val}")
print(f"Verified sysctl net.ipv4.ip_forward updated to {new_val}")

# 1.3 Rollback KernelParamSet
r1_rb = send_cap({
    "capability": "kernel_param_set",
    "key": "net.ipv4.ip_forward",
    "value": baseline,
    "persistent": False,
    "approved": True,
}, is_rb=True)
print("KernelParamSet rollback response:", r1_rb)
if r1_rb.get("status") != "success":
    sys.exit(f"FAIL: KernelParamSet rollback failed: {r1_rb}")

reverted_val = subprocess.check_output(["sysctl", "-n", "net.ipv4.ip_forward"]).decode().strip()
if reverted_val != baseline:
    sys.exit(f"FAIL: net.ipv4.ip_forward rollback expected {baseline}, got {reverted_val}")
print(f"Verified sysctl net.ipv4.ip_forward successfully reverted to baseline {reverted_val}")

print("==> 2. Testing KernelModuleLoad & Refcount-Guarded KernelModuleUnload")
# 2.1 Load allowlisted "dummy" module
r2 = send_cap({
    "capability": "kernel_module_load",
    "module": "dummy",
    "args": ["numdummies=2"],
    "approved": True,
})
print("KernelModuleLoad dummy response:", r2)
if r2.get("status") != "success":
    sys.exit(f"FAIL: KernelModuleLoad dummy failed: {r2}")

lsmod_out = subprocess.check_output(["lsmod"]).decode()
if "dummy" not in lsmod_out:
    sys.exit("FAIL: dummy module not found in lsmod after load")
print("Verified dummy module is loaded in kernel")

# 2.2 Increment refcount by bringing dummy0 interface UP
print("Bringing dummy0 link UP to increment module refcount...")
subprocess.check_call(["sudo", "ip", "link", "set", "dummy0", "up"])

# 2.3 Attempt to unload while in use -> MUST be denied by refcount guard
r2_denied = send_cap({
    "capability": "kernel_module_unload",
    "module": "dummy",
    "approved": True,
})
print("KernelModuleUnload (while in-use) response:", r2_denied)
if r2_denied.get("status") != "denied" or "in use" not in r2_denied.get("reason", ""):
    sys.exit(f"FAIL: Expected refcount guard denial for active module, got: {r2_denied}")
print("Verified Refcount Guard successfully denied unloading active module in-use")

# 2.4 Bring interface down and unload
print("Bringing dummy0 link DOWN to release module refcount...")
subprocess.check_call(["sudo", "ip", "link", "set", "dummy0", "down"])

r2_unload = send_cap({
    "capability": "kernel_module_unload",
    "module": "dummy",
    "approved": True,
}, is_rb=True)
print("KernelModuleUnload response:", r2_unload)
if r2_unload.get("status") != "success":
    sys.exit(f"FAIL: KernelModuleUnload failed: {r2_unload}")

lsmod_after = subprocess.check_output(["lsmod"]).decode()
if "dummy " in lsmod_after:
    sys.exit("FAIL: dummy module still present in lsmod after unload")
print("Verified dummy module cleanly unloaded from kernel")

print("==> 3. Testing Security Constraints & Allowlist / Denylist Refusals")
# 3.1 Prohibited parameter refusal
r3_prohibited = send_cap({
    "capability": "kernel_param_set",
    "key": "kernel.core_pattern",
    "value": "|/tmp/malicious",
    "persistent": False,
    "approved": True,
})
print("Prohibited kernel.core_pattern response:", r3_prohibited)
if r3_prohibited.get("status") != "denied":
    sys.exit(f"FAIL: Expected prohibited parameter denial, got: {r3_prohibited}")
print("Verified prohibited security parameter was denied")

# 3.2 Unallowed module refusal
r3_unallowed = send_cap({
    "capability": "kernel_module_load",
    "module": "unauthorized_test_module",
    "args": [],
    "approved": True,
})
print("Unallowed module response:", r3_unallowed)
if r3_unallowed.get("status") != "denied":
    sys.exit(f"FAIL: Expected unallowed module denial, got: {r3_unallowed}")
print("Verified unallowed module was denied by allowlist")

print("==> ALL PHASE 3.0.4 TIER 2 CAPABILITY IN-GUEST TESTS PASSED")
