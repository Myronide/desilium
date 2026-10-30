//! Placing a window at an exact rectangle.
//!
//! The KDE window protocols can move a window to a desktop but cannot set its geometry, and
//! session restore needs exactly that. KWin's scripting API can, so this loads a one-line
//! script through `org.kde.KWin /Scripting`, runs it and unloads it again. The script finds
//! the window by the uuid the window protocol reports.
//!
//! The scripting D-Bus interface and the `frameGeometry` property follow KWin 6 and could not
//! be checked against a running KWin when this was written.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use zbus::blocking::{Connection, Proxy};

use crate::error::KwinError;

const SERVICE: &str = "org.kde.KWin";
const SCRIPTING_PATH: &str = "/Scripting";
const SCRIPTING_INTERFACE: &str = "org.kde.kwin.Scripting";
const SCRIPT_INTERFACE: &str = "org.kde.kwin.Script";
const PLUGIN_PREFIX: &str = "desilium-geometry-";
const DIRECTORY: &str = "desilium";
/// Only the characters of a hyphenated uuid, braces tolerated; anything else is refused so the
/// uuid can be put into script source safely.
const UUID_MAX_LEN: usize = 40;

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Moves and resizes the window with `uuid` to `rect` = `(x, y, width, height)`.
///
/// Blocking and meant for a helper thread: it talks to the session bus and touches the disk.
pub fn set_geometry(uuid: &str, rect: (i32, i32, i32, i32)) -> Result<(), KwinError> {
    let source = geometry_script(uuid, rect)?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(KwinError::NoRuntimeDir)?;
    let directory = PathBuf::from(runtime).join(DIRECTORY);
    fs::create_dir_all(&directory)?;
    
    let number = COUNTER.fetch_add(1, Ordering::Relaxed);
    let plugin = format!("{PLUGIN_PREFIX}{}-{number}", std::process::id());
    let file = directory.join(format!("{plugin}.js"));
    fs::write(&file, source)?;
    
    let result = run_script(&file, &plugin);
    let _ = fs::remove_file(&file);
    result
}

fn run_script(file: &std::path::Path, plugin: &str) -> Result<(), KwinError> {
    let connection = Connection::session()?;
    let scripting = Proxy::new(&connection, SERVICE, SCRIPTING_PATH, SCRIPTING_INTERFACE)?;
    
    let id: i32 = scripting.call("loadScript", &(file.to_string_lossy().as_ref(), plugin))?;
    if id < 0 {
        return Err(KwinError::ScriptRejected(format!(
            "loadScript returned {id} for {plugin}"
        )));
    }
    
    let path = format!("{SCRIPTING_PATH}/Script{id}");
    let run = Proxy::new(&connection, SERVICE, path, SCRIPT_INTERFACE)
        .and_then(|script| script.call_method("run", &()).map(|_| ()));
    
    let unloaded: Result<bool, _> = scripting.call("unloadScript", &(plugin,));
    if let Err(error) = unloaded {
        tracing::debug!(%error, plugin, "script could not be unloaded");
    }
    Ok(run?)
}

/// The JavaScript that places the window. Pure so it can be tested.
pub fn geometry_script(uuid: &str, rect: (i32, i32, i32, i32)) -> Result<String, KwinError> {
    let bare = uuid.trim_matches(|c| c == '{' || c == '}');
    let valid = !bare.is_empty()
        && bare.len() <= UUID_MAX_LEN
        && bare.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if !valid {
        return Err(KwinError::BadUuid(uuid.to_owned()));
    }
    let (x, y, width, height) = rect;
    let (width, height) = (width.max(1), height.max(1));
    Ok(format!(
        "const wanted = \"{bare}\";\n\
         for (const window of workspace.windowList()) {{\n\
         \x20   if (window.internalId.toString().replace(/[{{}}]/g, \"\") === wanted) {{\n\
         \x20       window.frameGeometry = {{ x: {x}, y: {y}, width: {width}, height: {height} }};\n\
         \x20       break;\n\
         \x20   }}\n\
         }}\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "6f1c1c0e-47b3-4a51-9b54-0a3a2b3f9d10";

    #[test]
    fn script_names_the_window_and_the_rectangle() {
        let script = geometry_script(UUID, (10, -20, 800, 600)).expect("valid");
        assert!(script.contains(&format!("wanted = \"{UUID}\"")));
        assert!(script.contains("x: 10, y: -20, width: 800, height: 600"));
        assert!(script.contains("workspace.windowList()"));
    }

    #[test]
    fn braces_around_the_uuid_are_accepted_and_dropped() {
        let script = geometry_script(&format!("{{{UUID}}}"), (0, 0, 1, 1)).expect("valid");
        assert!(script.contains(&format!("wanted = \"{UUID}\"")));
    }

    #[test]
    fn empty_sizes_are_raised_to_one_pixel() {
        let script = geometry_script(UUID, (0, 0, 0, -5)).expect("valid");
        assert!(script.contains("width: 1, height: 1"));
    }

    #[test]
    fn anything_that_could_break_out_of_the_string_is_refused() {
        for bad in [
            "",
            "{}",
            "abc\"; evil(); \"",
            "not a uuid",
            "6f1c1c0e-47b3-4a51-9b54-0a3a2b3f9d10-and-much-more-than-forty-chars",
            "g0000000-0000-0000-0000-000000000000",
        ] {
            assert!(
                matches!(
                    geometry_script(bad, (0, 0, 1, 1)),
                    Err(KwinError::BadUuid(_))
                ),
                "{bad:?}"
            );
        }
    }
}
