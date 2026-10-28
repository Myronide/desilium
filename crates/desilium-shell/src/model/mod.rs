//! Pure data model of the shell: windows, dock content, launcher state, clock, battery.
//!
//! Nothing in here touches Wayland, the file system or the clock, so all of it is unit tested.

pub(crate) mod battery;
pub(crate) mod clock;
pub(crate) mod desktop;
pub(crate) mod dock;
pub(crate) mod grid;
pub(crate) mod launcher;
pub(crate) mod pins;
pub(crate) mod windows;
