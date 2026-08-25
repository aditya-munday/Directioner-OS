# polkit rules for AI Engineer operations.
#
# Approval gates in the AI Control Plane are logical; polkit is the OS-level
# equivalent for user-initiated privileged actions (e.g., installing a package
# the user asked for directly in the GUI). Rules here must be conservative:
# - package install/remove  -> auth_admin (password required)
# - service restart (non-critical) -> auth_admin_keep
# - any kernel/boot change -> require explicit expert-review tier
#
# TODO(Phase 0): add .rules files once the capability names are finalized.
# Keep the pairing explicit: one polkit rule per typed capability, never a
# blanket "allow all" for ai-* identities.