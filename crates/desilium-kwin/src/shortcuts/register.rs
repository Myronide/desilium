//! Registers key bindings with `kglobalacceld` over D-Bus.

use zbus::blocking::{Connection, Proxy};

use super::{Binding, Target};
use crate::error::KwinError;

const SERVICE: &str = "org.kde.kglobalaccel";
const PATH: &str = "/kglobalaccel";
const INTERFACE: &str = "org.kde.KGlobalAccel";
/// `KGlobalAccel::SetShortcutFlag::SetPresent`: the action exists in the running application.
const SET_PRESENT: u32 = 0x2;
/// `KGlobalAccel::SetShortcutFlag::NoAutoloading`: the given keys win over the stored ones.
const NO_AUTOLOADING: u32 = 0x4;
const KWIN_FRIENDLY: &str = "KWin";
const DESILIUM_FRIENDLY: &str = "Desilium";

/// The four strings KGlobalAccel identifies an action by.
fn action_id(target: Target) -> Vec<String> {
    let (component, action) = target.names();
    let component_friendly = match target {
        Target::Kwin(_) => KWIN_FRIENDLY,
        Target::Desilium(_) => DESILIUM_FRIENDLY,
    };
    vec![
        component.to_owned(),
        action.to_owned(),
        component_friendly.to_owned(),
        target.friendly_name().to_owned(),
    ]
}

/// Registers every binding. Returns how many could not be set; a missing `kglobalacceld`
/// is an error because then nothing can be set.
pub fn register(connection: &Connection, bindings: &[Binding]) -> Result<usize, KwinError> {
    let proxy = Proxy::new(connection, SERVICE, PATH, INTERFACE)?;
    let mut failed = 0;
    for binding in bindings {
        match register_one(&proxy, binding) {
            Ok(()) => {}
            // The daemon is not up: stop instead of logging one failure per binding. The caller
            // retries later.
            Err(error) if daemon_missing(&error) => return Err(error),
            Err(error) => {
                tracing::warn!(target = ?binding.target, %error, "shortcut was not registered");
                failed += 1;
            }
        }
    }
    Ok(failed)
}

/// The error says that nobody owns the `kglobalacceld` name.
fn daemon_missing(error: &KwinError) -> bool {
    match error {
        KwinError::Service(
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_),
        ) => true,
        KwinError::Bus(zbus::Error::MethodError(name, _, _)) => {
            name.ends_with("ServiceUnknown") || name.ends_with("NameHasNoOwner")
        }
        _ => false,
    }
}

fn register_one(proxy: &Proxy<'_>, binding: &Binding) -> Result<(), KwinError> {
    let id = action_id(binding.target);
    match binding.target {
        Target::Desilium(_) => {
            proxy.call_method("doRegister", &(&id,))?;
            let flags = SET_PRESENT | NO_AUTOLOADING;
            let _applied: Vec<i32> = proxy.call("setShortcut", &(&id, &binding.keys, flags))?;
        }
        Target::Kwin(_) => {
            proxy.call_method("setForeignShortcut", &(&id, &binding.keys))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::TOGGLE_LAUNCHER;

    #[test]
    fn action_ids_have_the_four_fields_in_kglobalaccel_order() {
        let id = action_id(Target::Desilium(TOGGLE_LAUNCHER));
        assert_eq!(
            id,
            vec![
                "desilium",
                "toggle-launcher",
                "Desilium",
                "Show or hide the launcher"
            ]
        );
        let kwin = action_id(Target::Kwin("Window Close"));
        assert_eq!(kwin, vec!["kwin", "Window Close", "KWin", "Window Close"]);
    }

    #[test]
    fn flags_combine_to_six() {
        assert_eq!(SET_PRESENT | NO_AUTOLOADING, 6);
    }
}
