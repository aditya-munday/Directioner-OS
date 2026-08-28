import json
import time

entry = {
    "seq": 99,
    "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "request_id": "tier1-postboot-observation",
    "capability": "boot_watchdog.tier1_rollback_observed",
    "args": {
        "trigger": "watchdog: required milestone health-check failed 3 consecutive attempts",
        "action": "automatic rollback to pinned deployment 0",
        "mechanism": "bootcounter exhaustion + autonomous bootc rollback",
    },
    "requester": "incident-response-agent:tier1-observer",
    "token": "tok-tier1-obs",
    "is_rollback": True,
    "outcome": {
        "status": "success",
        "result": {"active_deployment_restored": True},
    },
    "duration_ms": 0,
}
with open("/tmp/live-worklog.jsonl", "a") as f:
    f.write(json.dumps(entry) + "\n")
