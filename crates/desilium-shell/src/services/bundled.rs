//! Resolve desktop-owned apps next to the shell, without installing into the host session.
use desilium_apps::{
    Catalog, DesktopEntry, DesktopId, Environment, desktop::ini, env::is_executable,
};
use std::path::Path;
const BUNDLED: [(&str, &str, &str); 2] = [
    (
        "doria",
        "org.desilium.Doria.desktop",
        include_str!("../../../../packaging/applications/org.desilium.Doria.desktop"),
    ),
    (
        "desilium-settings",
        "desilium-settings.desktop",
        include_str!("../../../../packaging/applications/desilium-settings.desktop"),
    ),
];
pub(crate) fn catalog(env: &Environment, binary_dir: Option<&Path>) -> Catalog {
    let mut catalog = Catalog::load(env);
    for (binary, id, raw) in BUNDLED {
        let program = binary_dir
            .map(|dir| dir.join(binary))
            .filter(|path| is_executable(path))
            .or_else(|| env.find_program(binary));
        let Some(program) = program else {
            continue;
        };
        let Some(id) = DesktopId::new(id) else {
            continue;
        };
        let Ok(file) = ini::parse(raw) else {
            continue;
        };
        let Ok(mut entry) = DesktopEntry::from_ini(id, program.clone(), &file, &env.locales) else {
            continue;
        };
        let Some(path) = program
            .to_str()
            .filter(|path| !path.chars().any(char::is_control))
        else {
            continue;
        };
        entry.exec = format!(
            "\"{}\"",
            path.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('%', "%%")
        );
        entry.try_exec = Some(path.into());
        catalog.add_fallback(entry);
    }
    catalog
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn doria_is_discoverable_and_launchable_without_installation() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("build with spaces");
        fs::create_dir(&bin).unwrap();
        fs::write(bin.join("doria"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(bin.join("doria"), fs::Permissions::from_mode(0o755)).unwrap();
        let env = Environment::from_lookup(|key| match key {
            "HOME" => Some(dir.path().display().to_string()),
            "XDG_DATA_DIRS" => Some(dir.path().join("empty").display().to_string()),
            _ => None,
        });
        let mut catalog = catalog(&env, Some(&bin));
        let entry = catalog.get("org.desilium.Doria.desktop").unwrap();
        assert!(catalog.is_listed(entry.id.as_str()));
        let command =
            desilium_apps::launch::for_entry(entry, None, &Default::default(), None).unwrap();
        assert_eq!(Path::new(&command.program), bin.join("doria"));
        fs::create_dir_all(dir.path().join("empty/applications")).unwrap();
        assert!(catalog.refresh_if_changed());
        assert!(catalog.is_listed("org.desilium.Doria.desktop"));
    }
    #[test]
    fn explicit_hidden_xdg_entry_keeps_bundled_doria_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("applications");
        fs::create_dir(&apps).unwrap();
        fs::write(dir.path().join("doria"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(dir.path().join("doria"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            apps.join("org.desilium.Doria.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=doria\nHidden=true\n",
        )
        .unwrap();
        let env = Environment::from_lookup(|key| match key {
            "HOME" => Some(dir.path().display().to_string()),
            "XDG_DATA_DIRS" => Some(dir.path().display().to_string()),
            _ => None,
        });
        assert!(!catalog(&env, Some(dir.path())).is_listed("org.desilium.Doria.desktop"));
    }
}
