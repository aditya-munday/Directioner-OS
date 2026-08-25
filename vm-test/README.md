# vm-test — Disposable VM test harness

The rule from the build guide (§4): nothing touches a real machine before it has
been proven in a disposable VM. Every benchmark scenario follows the same pattern:

    fresh disposable qcow2 snapshot → run the scenario → score → discard.

## Quick loop (guide §4.1)

```sh
# snapshot a known-good base image once (one-time)
qemu-img create -f qcow2 -b output/qcow2/disk.qcow2 -F qcow2 vm-test/run.qcow2

# boot it, run the scenario, throw it away
qemu-system-x86_64 -m 4096 -enable-kvm \
  -drive file=vm-test/run.qcow2,format=qcow2

rm vm-test/run.qcow2   # disposable — next run starts clean again
```

## What must pass before anything runs on real hardware (guide §4.2)

- [ ] Benchmark Suite score has not regressed vs. the last release run on real hardware
- [ ] The action class being tried has an automated test in the VM harness
- [ ] Kill-switch exercised at least once this session
- [ ] `bootc rollback` tested, not assumed (required before Phase 3 work)

## Staged rollout habit (guide §4.3), even solo

1. Disposable VM first, always.
2. Secondary/spare machine or mirror-config VM next, if available.
3. Real machine only with the dry-run/preview step actually read.
4. Pin the previous bootc deployment until the new one has been used ≥ 1 day.

Future phases will add scenario runners here:

- `scenarios/` — one directory per benchmark task (setup script, task prompt, checker)
- `scored/` — pass/fail result JSON the Benchmark Suite aggregates
- Phase 3 will add fault injection (break a service / boot slot, measure MTTR to rollback)

## CI smoke contract

The CI `vm-smoke` job boots the produced qcow2 headlessly with user-mode
networking (port 2222 → guest 22), waits for SSH using the disposable `ci`
user, then asserts over SSH: the custom motd is present, systemd reached
`running`/`degraded`, and `bootc --version` works. That is the guide §2.3
contract: "SSH in, check a systemd unit is active."

## About `ci_key` / `ci_key.pub`

A throwaway ed25519 keypair whose public half is injected into CI-built images
(`config.toml` at build time, user `ci`). It exists only so the headless VM
smoke test can SSH into a freshly booted disposable VM. It grants access to
nothing else — never reuse it outside this repo's CI.