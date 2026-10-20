//! Directioner OS User-Session Executor (directioner-actd)
//!
//! Provides user-scope capability execution (U072), polkit action mapping (U073),
//! dry-run preview engine (U074), and persistent undo journal (U075).

pub mod backends;
pub mod polkit;
pub mod preview;
pub mod server;
pub mod undo;

pub use backends::{ActionError, UserExecutor};
pub use polkit::PolkitMap;
pub use preview::{ActionRisk, PreviewEngine, PreviewPlan};
pub use server::{ActDaemon, ActRequest, ActResponse};
pub use undo::{UndoJournal, UndoJournalEntry};
