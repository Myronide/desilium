//! Pure data model of the shell: windows, dock content, launcher state, clock, battery.
//!
//! Nothing in here touches Wayland, the file system or the clock, so all of it is unit tested.

pub mod battery;
pub mod clock;
pub mod dock;
pub mod icons;
pub mod launcher;
pub mod pins;
pub mod windows;
