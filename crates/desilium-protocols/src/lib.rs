//! Private Wayland protocols of Desilium, generated from the XML in `protocols/`.
//!
//! Enable the `server` feature in the compositor and the `client` feature in the shell.

/// The shell protocol XML, for tests and tooling.
pub const SHELL_PROTOCOL_XML: &str = include_str!("../protocols/desilium-shell-unstable-v1.xml");

/// Name of the global that carries the shell protocol.
pub const SHELL_MANAGER_INTERFACE: &str = "desilium_shell_manager_v1";

/// Names of the compositor key bindings that the shell is told about.
pub mod shortcut {
    /// Show or hide the application launcher.
    pub const TOGGLE_LAUNCHER: &str = "toggle_launcher";
}

#[cfg(feature = "server")]
#[allow(missing_docs, unused_imports, non_camel_case_types)]
pub mod server {
    //! Server-side bindings of `desilium-shell-v1`.
    use wayland_server;
    use wayland_server::protocol::*;

    #[allow(missing_docs)]
    pub mod interfaces {
        use wayland_server::backend as wayland_backend;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/desilium-shell-unstable-v1.xml");
    }
    use self::interfaces::*;

    wayland_scanner::generate_server_code!("protocols/desilium-shell-unstable-v1.xml");
}

#[cfg(feature = "client")]
#[allow(missing_docs, unused_imports, non_camel_case_types)]
pub mod client {
    //! Client-side bindings of `desilium-shell-v1`.
    use wayland_client;
    use wayland_client::protocol::*;

    #[allow(missing_docs)]
    pub mod interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/desilium-shell-unstable-v1.xml");
    }
    use self::interfaces::*;

    wayland_scanner::generate_client_code!("protocols/desilium-shell-unstable-v1.xml");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_declares_both_interfaces_at_version_one() {
        assert!(SHELL_PROTOCOL_XML.contains(r#"name="desilium_shell_manager_v1" version="1""#));
        assert!(SHELL_PROTOCOL_XML.contains(r#"name="desilium_toplevel_handle_v1" version="1""#));
        assert!(SHELL_PROTOCOL_XML.contains(SHELL_MANAGER_INTERFACE));
    }

    #[cfg(all(feature = "server", feature = "client"))]
    #[test]
    fn bindings_for_both_sides_link() {
        let server = &server::interfaces::DESILIUM_SHELL_MANAGER_V1_INTERFACE;
        let client = &client::interfaces::DESILIUM_SHELL_MANAGER_V1_INTERFACE;
        assert_eq!(server.name, SHELL_MANAGER_INTERFACE);
        assert_eq!(server.name, client.name);
    }
}
