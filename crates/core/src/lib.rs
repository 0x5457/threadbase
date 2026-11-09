//! Pure domain policy and event reduction for Threadbase.

pub mod delivery;
pub mod permissions;
pub mod reducer;

pub use delivery::{resolve_delivery, DeliveryContext, SendAction};
pub use permissions::{
    can_manage, can_write, require_manage, require_write, rule_inherits_to_child, rule_matches,
    scope_is_reusable, PolicyError,
};
pub use reducer::{ThinkingBlock, TimelineState};
