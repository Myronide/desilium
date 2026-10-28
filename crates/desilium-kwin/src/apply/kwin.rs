//! Calls into the running KWin over D-Bus.

use zbus::blocking::{Connection, Proxy};

use crate::error::KwinError;

const SERVICE: &str = "org.kde.KWin";
const DESKTOPS_PATH: &str = "/VirtualDesktopManager";
const DESKTOPS_INTERFACE: &str = "org.kde.KWin.VirtualDesktopManager";
/// One row: the desktops sit next to each other, as the bar's desktop switcher shows them.
const DESKTOP_ROWS: u32 = 1;

/// Asks KWin to re-read its configuration (`kwinrc` and what it watches).
pub fn reconfigure(connection: &Connection) -> Result<(), KwinError> {
    let proxy = Proxy::new(connection, SERVICE, "/KWin", "org.kde.KWin")?;
    proxy.call_method("reconfigure", &())?;
    Ok(())
}

/// Makes KWin have `count` virtual desktops in one row. Does nothing when it already has them.
pub fn set_desktop_count(connection: &Connection, count: u8) -> Result<(), KwinError> {
    let proxy = Proxy::new(connection, SERVICE, DESKTOPS_PATH, DESKTOPS_INTERFACE)?;
    let wanted = u32::from(count);
    let current: u32 = proxy.get_property("count")?;
    if current != wanted {
        proxy.set_property("count", wanted)?;
    }
    let rows: u32 = proxy.get_property("rows")?;
    if rows != DESKTOP_ROWS {
        proxy.set_property("rows", DESKTOP_ROWS)?;
    }
    Ok(())
}
