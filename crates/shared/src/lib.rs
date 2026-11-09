//! Wire and domain types shared by every Threadbase crate and the web client.

pub mod api;
pub mod error;
pub mod event;
pub mod focus;
pub mod ids;
pub mod model;

pub use api::*;
pub use error::{ApiError, ErrorCode};
pub use event::*;
pub use focus::*;
pub use ids::*;
pub use model::*;

/// Milliseconds since the Unix epoch.
#[must_use]
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

/// Stable color assignment shared by presence cursors and draft awareness.
#[must_use]
pub fn user_color(user_id: &str) -> &'static str {
    const PALETTE: [&str; 8] = [
        "#f97316", "#ec4899", "#84cc16", "#a855f7", "#eab308", "#fb7185", "#3b82f6", "#94a3b8",
    ];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in user_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    PALETTE[(hash as usize) % PALETTE.len()]
}
