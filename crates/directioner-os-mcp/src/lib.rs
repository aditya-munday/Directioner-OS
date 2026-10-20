//! Directioner OS Model Context Protocol Server (directioner-os-mcp)
//!
//! Exposes capability catalog (U082), live observe plane state, dry-run previews (U074),
//! and D-Bus discovery surface (U085) to AI agents via standard MCP protocol.

pub mod handler;
pub mod protocol;

pub use handler::McpHandler;
pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpTool};
