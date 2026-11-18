//! MTProto Unified Connection Types and Re-exports
pub use crate::mtproto::connection_abstract::*;
pub use crate::mtproto::connection_tcp::*;
pub use crate::mtproto::connection_http::*;
pub use crate::mtproto::connection_resolving::*;

pub type Connection = ConnectionAbstract;
