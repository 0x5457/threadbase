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
    jiff::Timestamp::now().as_millisecond()
}

/// Stable color assignment shared by presence cursors and draft awareness.
#[must_use]
pub fn user_color(user_id: &str) -> &'static str {
    // Zed's collaborator `players` palette (One Dark); the web client keeps the
    // same order in `USER_COLORS`.
    const PALETTE: [&str; 8] = [
        "#74ade8", "#be5046", "#bf956a", "#b477cf", "#6eb4bf", "#d07277", "#dec184", "#a1c181",
    ];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in user_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    PALETTE[(hash as usize) % PALETTE.len()]
}
