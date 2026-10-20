//! Directioner OS Reference Control Fabric CLI (dosctl)
//!
//! Provides the primary command-line interface to observe state (U079),
//! query capability catalog (U063), generate dry-run previews (U074),
//! submit actions, perform undo operations (U075), and introspect D-Bus (U085).

pub mod client;
pub mod dbus;

pub use client::FabricClient;
pub use dbus::{DbusIntrospector, DbusNode};
