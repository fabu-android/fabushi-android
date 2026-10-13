//! Public, surface-neutral contracts for conversational MCP mini apps.

mod compiler;
mod payment;
mod types;

pub use compiler::{CompiledContent, ContentCompiler, ContentSource, SourceKind};
pub use payment::{
    CheckoutStatus, CreatePaymentRequest, OpenCheckoutRequest, PaymentContractError,
    PaymentStatusRequest, PaymentView, PAYMENT_SCHEMA,
};
pub use types::{
    extract_structured_content, mcp_result_text, AppSummary, ChatDisposition, ContentReceipt,
    Feed, FeedItem, FeedItemKind, HomeDocument, HomeRequest, QuickAction, QuickReply, RichMessage,
    Tip, Welcome, HOME_SCHEMA, MAX_HOME_BYTES, MAX_HOME_FEED_ITEMS,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
