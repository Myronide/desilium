//! Pure animation logic for the compositor: clocks, schedules, transforms and the per-window
//! state machine. Nothing here touches Smithay or the GPU, so all of it is unit-testable.

mod book;
mod clock;
mod dev;
mod motion;
mod timing;
mod xform;

pub use book::MotionBook;
pub use clock::Clock;
pub use dev::{Demo, parse_freeze};
pub use motion::{ButtonFade, Observation, Pose, Presence};
pub use timing::{Freeze, Pace, Timing};
pub use xform::{RectF, Xform};
