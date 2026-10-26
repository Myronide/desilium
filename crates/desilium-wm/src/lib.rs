//! Smithay-free floating window management: stacking, focus, placement, resize anchoring,
//! window states, snapping and virtual desktops.

mod desktops;
mod geometry;
mod ids;
mod manager;
mod resize;
mod snap;
mod state;
mod workspace;

pub use desktops::{DEFAULT_DESKTOPS, Desktops, MAX_DESKTOPS, MIN_DESKTOPS};
pub use geometry::{Point, Rect, Size};
pub use ids::WindowId;
pub use manager::{Placement, WindowManager};
pub use resize::{Edges, SizeLimits, anchored_location, resized_size};
pub use snap::{SnapZone, snap_preview, snap_rect, snap_zone_at};
pub use state::WindowState;
