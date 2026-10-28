//! The resolved session plan: which components start, in which order, with which environment.
//!
//! Building a plan is pure apart from the file checks in `defaults`; it spawns nothing, so the
//! same plan can be printed by `--dry-run` and then executed by the supervisor.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::component::{ComponentSpec, Criticality, EnvVar, WaylandRole};
use crate::startup::dbus::{
    BUS_ADDRESS_VARIABLE, DbusAction, DbusInput, REEXEC_MARKER_VARIABLE, ensure_dbus_session_bus,
};
use crate::startup::defaults::{
    DEFAULT_COMPONENTS, DefaultComponent, FoundVia, find_default, find_in_path, resolve_explicit,
};
use crate::startup::environment::{EnvChange, EnvEntry, Environment, session_environment};
use crate::startup::readiness::WaylandReadiness;

const LAUNCHER_BINARY: &str = "dbus-run-session";
const COMPOSITOR: &str = "compositor";
/// Variable that makes KWin hand its privileged protocols to every client (development only).
pub const KWIN_NO_PERMISSION_CHECKS: &str = "KWIN_WAYLAND_NO_PERMISSION_CHECKS";
/// What KWin is started with unless the command line adds more: a rootless Xwayland for X11
/// applications, and no lock screen (it needs `kscreenlocker`, which Desilium does not ship).
const KWIN_DEFAULT_ARGS: [&str; 2] = ["--xwayland", "--no-lockscreen"];

/// What the command line asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanRequest {
    /// Explicit KWin executable (`--compositor`).
    pub compositor: Option<PathBuf>,
    /// Explicit shell executable (`--shell`).
    pub shell: Option<PathBuf>,
    /// Explicit KWin bridge executable (`--kwin-bridge`).
    pub kwin_bridge: Option<PathBuf>,
    /// Explicit notification daemon executable (`--notifications`).
    pub notifications: Option<PathBuf>,
    /// Arguments passed through to KWin, after its defaults.
    pub compositor_args: Vec<OsString>,
    /// Development: let every client use KWin's privileged protocols (`--allow-any-client`).
    pub allow_any_client: bool,
}

impl PlanRequest {
    fn explicit(&self, name: &str) -> Option<&Path> {
        match name {
            COMPOSITOR => self.compositor.as_deref(),
            "shell" => self.shell.as_deref(),
            "kwin-bridge" => self.kwin_bridge.as_deref(),
            "notifications" => self.notifications.as_deref(),
            _ => None,
        }
    }
}

/// A component that will be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedComponent {
    /// Name used in logs and plans.
    pub name: &'static str,
    /// Executable that will be started.
    pub program: PathBuf,
    /// How the executable was located.
    pub found_via: FoundVia,
    /// Restart behaviour.
    pub criticality: Criticality,
    /// Relation to the Wayland socket.
    pub wayland: WaylandRole,
    /// Arguments for the component.
    pub args: Vec<OsString>,
}

/// A default component that is not installed and therefore skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkippedComponent {
    /// Name of the skipped component.
    pub name: &'static str,
    /// Binary that was looked for.
    pub binary: &'static str,
}

/// Reasons a plan cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    /// A required component was not found next to the session or in `PATH`.
    #[error(
        "required component `{name}` was not found (looked for `{binary}` next to desilium-session and in PATH)"
    )]
    NotFound {
        name: &'static str,
        binary: &'static str,
    },
    /// A component was given explicitly but the file does not exist.
    #[error("component `{name}` was given as {path:?}, which does not exist")]
    ExplicitMissing { name: &'static str, path: PathBuf },
}

/// The complete, resolved start-up plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    /// Components in start order.
    pub components: Vec<PlannedComponent>,
    /// Default components that were not installed.
    pub skipped: Vec<SkippedComponent>,
    /// What to do about the D-Bus session bus.
    pub dbus: DbusAction,
    /// Session environment, with the user's own values marked as kept.
    pub env: Vec<EnvEntry>,
    /// `XDG_RUNTIME_DIR`, where the Wayland socket appears.
    pub runtime_dir: Option<PathBuf>,
}

/// Resolves the components, environment and D-Bus action for one session start.
pub fn build_plan(
    request: &PlanRequest,
    env: &Environment,
    exe_dir: Option<&Path>,
) -> Result<SessionPlan, PlanError> {
    let search_path = env.search_path();
    let mut components = Vec::new();
    let mut skipped = Vec::new();
    for default in &DEFAULT_COMPONENTS {
        match locate(
            default,
            request.explicit(default.name),
            exe_dir,
            search_path,
        )? {
            Some((program, found_via)) => {
                components.push(planned(default, program, found_via, request));
            }
            None => skipped.push(SkippedComponent {
                name: default.name,
                binary: default.binary,
            }),
        }
    }
    let dbus = ensure_dbus_session_bus(&DbusInput {
        bus_address_set: env.non_empty(BUS_ADDRESS_VARIABLE).is_some(),
        already_reexecuted: env.non_empty(REEXEC_MARKER_VARIABLE).is_some(),
        launcher: find_in_path(Path::new(LAUNCHER_BINARY), search_path),
    });
    let mut session_env = session_environment(env);
    if request.allow_any_client {
        session_env.push(EnvEntry {
            key: KWIN_NO_PERMISSION_CHECKS,
            change: EnvChange::Set {
                value: OsString::from("1"),
                previous: env.get(KWIN_NO_PERMISSION_CHECKS).cloned(),
            },
        });
    }
    Ok(SessionPlan {
        components,
        skipped,
        dbus,
        env: session_env,
        runtime_dir: env.runtime_dir(),
    })
}

/// Finds one component: an explicit flag must exist; otherwise discovery decides whether it is required.
fn locate(
    default: &DefaultComponent,
    explicit: Option<&Path>,
    exe_dir: Option<&Path>,
    search_path: Option<&std::ffi::OsStr>,
) -> Result<Option<(PathBuf, FoundVia)>, PlanError> {
    if let Some(path) = explicit {
        return resolve_explicit(path, search_path)
            .map(|found| Some((found, FoundVia::Flag)))
            .ok_or_else(|| PlanError::ExplicitMissing {
                name: default.name,
                path: path.to_path_buf(),
            });
    }
    match find_default(default.binary, exe_dir, search_path) {
        Some(found) => Ok(Some(found)),
        None if default.required => Err(PlanError::NotFound {
            name: default.name,
            binary: default.binary,
        }),
        None => Ok(None),
    }
}

fn planned(
    default: &DefaultComponent,
    program: PathBuf,
    found_via: FoundVia,
    request: &PlanRequest,
) -> PlannedComponent {
    let args = if default.name == COMPOSITOR {
        let mut args: Vec<OsString> = KWIN_DEFAULT_ARGS.iter().map(OsString::from).collect();
        args.extend(request.compositor_args.iter().cloned());
        args
    } else {
        Vec::new()
    };
    PlannedComponent {
        name: default.name,
        program,
        found_via,
        criticality: default.criticality,
        wayland: default.wayland,
        args,
    }
}

impl SessionPlan {
    /// Environment entries that are exported to the children.
    pub fn exported(&self) -> impl Iterator<Item = &EnvEntry> {
        self.env
            .iter()
            .filter(|entry| matches!(entry.change, EnvChange::Set { .. }))
    }

    /// Supervisor specs for the components, in start order.
    pub fn component_specs(&self) -> Vec<ComponentSpec> {
        let env: Vec<EnvVar> = self
            .exported()
            .map(|entry| (OsString::from(entry.key), entry.value().clone()))
            .collect();
        self.components
            .iter()
            .map(|component| {
                let mut spec = ComponentSpec::new(
                    component.name,
                    component.program.clone(),
                    component.criticality,
                );
                spec.args = component.args.clone();
                spec.env = env.clone();
                spec.wayland = component.wayland;
                spec
            })
            .collect()
    }

    /// Wayland readiness settings, when the runtime directory is known.
    pub fn readiness(&self) -> Option<WaylandReadiness> {
        self.runtime_dir.clone().map(WaylandReadiness::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::scratch_tests::Scratch;

    fn pair(key: &str, value: &str) -> (OsString, OsString) {
        (OsString::from(key), OsString::from(value))
    }

    fn base_env(dirs: &[PathBuf]) -> Environment {
        let path = std::env::join_paths(dirs).expect("joinable");
        Environment::new([
            pair("PATH", path.to_str().expect("utf-8 path")),
            pair("XDG_RUNTIME_DIR", "/run/user/1000"),
            pair("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
        ])
    }

    fn install_all(scratch: &Scratch) {
        for binary in [
            "kwin_wayland",
            "desilium-shell",
            "desilium-kwin",
            "desilium-notifications",
        ] {
            scratch.touch(&format!("bin/{binary}"));
        }
    }

    fn names(plan: &SessionPlan) -> Vec<&'static str> {
        plan.components.iter().map(|c| c.name).collect()
    }

    #[test]
    fn full_install_starts_kwin_then_clients_next_to_the_session() {
        let scratch = Scratch::new("plan-full");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");

        let plan = build_plan(&PlanRequest::default(), &base_env(&[]), Some(&exe_dir))
            .expect("plan builds");

        assert_eq!(
            names(&plan),
            ["compositor", "shell", "kwin-bridge", "notifications"]
        );
        assert!(plan.skipped.is_empty());
        assert!(
            plan.components
                .iter()
                .all(|c| c.found_via == FoundVia::Sibling)
        );
        assert_eq!(plan.components[1].wayland, WaylandRole::Client);
        assert_eq!(plan.components[0].wayland, WaylandRole::Provider);
    }

    #[test]
    fn missing_optional_clients_are_skipped_and_reported() {
        let scratch = Scratch::new("plan-skip");
        scratch.touch("bin/kwin_wayland");
        let exe_dir = scratch.path().join("bin");

        let plan = build_plan(&PlanRequest::default(), &base_env(&[]), Some(&exe_dir))
            .expect("plan builds");

        assert_eq!(names(&plan), ["compositor"]);
        assert_eq!(
            plan.skipped,
            vec![
                SkippedComponent {
                    name: "shell",
                    binary: "desilium-shell",
                },
                SkippedComponent {
                    name: "kwin-bridge",
                    binary: "desilium-kwin",
                },
                SkippedComponent {
                    name: "notifications",
                    binary: "desilium-notifications",
                },
            ]
        );
    }

    #[test]
    fn missing_compositor_is_an_error() {
        let scratch = Scratch::new("plan-no-compositor");
        scratch.touch("bin/desilium-shell");
        let exe_dir = scratch.path().join("bin");

        let error = build_plan(&PlanRequest::default(), &base_env(&[]), Some(&exe_dir))
            .expect_err("compositor is required");

        assert_eq!(
            error,
            PlanError::NotFound {
                name: "compositor",
                binary: "kwin_wayland",
            }
        );
    }

    #[test]
    fn explicit_flag_overrides_discovery_and_must_exist() {
        let scratch = Scratch::new("plan-explicit");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");
        let custom = scratch.touch("custom/my-shell");
        let request = PlanRequest {
            shell: Some(custom.clone()),
            ..PlanRequest::default()
        };

        let plan = build_plan(&request, &base_env(&[]), Some(&exe_dir)).expect("plan builds");
        assert_eq!(plan.components[1].program, custom);
        assert_eq!(plan.components[1].found_via, FoundVia::Flag);

        let missing = PlanRequest {
            notifications: Some(scratch.path().join("gone")),
            ..PlanRequest::default()
        };
        let error = build_plan(&missing, &base_env(&[]), Some(&exe_dir))
            .expect_err("explicit path must exist");
        assert!(matches!(
            error,
            PlanError::ExplicitMissing {
                name: "notifications",
                ..
            }
        ));
    }

    #[test]
    fn kwin_gets_its_defaults_then_the_passthrough_arguments_and_nobody_else_does() {
        let scratch = Scratch::new("plan-args");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");
        let request = PlanRequest {
            compositor_args: vec![OsString::from("--virtual")],
            ..PlanRequest::default()
        };

        let plan = build_plan(&request, &base_env(&[]), Some(&exe_dir)).expect("plan builds");

        assert_eq!(
            plan.components[0].args,
            [
                OsString::from("--xwayland"),
                OsString::from("--no-lockscreen"),
                OsString::from("--virtual")
            ]
        );
        assert!(plan.components[1..].iter().all(|c| c.args.is_empty()));
    }

    #[test]
    fn allow_any_client_exports_the_kwin_switch_and_is_off_by_default() {
        let scratch = Scratch::new("plan-any-client");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");
        let default = build_plan(&PlanRequest::default(), &base_env(&[]), Some(&exe_dir))
            .expect("plan builds");
        assert!(
            default
                .env
                .iter()
                .all(|entry| entry.key != KWIN_NO_PERMISSION_CHECKS)
        );

        let request = PlanRequest {
            allow_any_client: true,
            ..PlanRequest::default()
        };
        let plan = build_plan(&request, &base_env(&[]), Some(&exe_dir)).expect("plan builds");
        let entry = plan
            .env
            .iter()
            .find(|entry| entry.key == KWIN_NO_PERMISSION_CHECKS)
            .expect("switch is exported");
        assert_eq!(entry.value(), "1");
    }

    #[test]
    fn path_fallback_is_used_when_nothing_sits_next_to_the_session() {
        let scratch = Scratch::new("plan-path");
        scratch.touch("usr/kwin_wayland");
        let env = base_env(&[scratch.path().join("usr")]);

        let plan = build_plan(&PlanRequest::default(), &env, None).expect("plan builds");

        assert_eq!(plan.components[0].found_via, FoundVia::Path);
    }

    #[test]
    fn dbus_is_reexec_only_when_no_bus_and_a_launcher_exists() {
        let scratch = Scratch::new("plan-dbus");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");
        scratch.touch("launcher/dbus-run-session");
        let launcher_dir = scratch.path().join("launcher");
        let env = Environment::new([
            pair("PATH", launcher_dir.to_str().expect("utf-8 path")),
            pair("XDG_RUNTIME_DIR", "/run/user/1000"),
        ]);

        let plan = build_plan(&PlanRequest::default(), &env, Some(&exe_dir)).expect("plan builds");

        assert_eq!(
            plan.dbus,
            DbusAction::ReexecUnder {
                launcher: launcher_dir.join("dbus-run-session"),
            }
        );
    }

    #[test]
    fn existing_bus_address_is_reused() {
        let scratch = Scratch::new("plan-bus-existing");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");

        let plan = build_plan(&PlanRequest::default(), &base_env(&[]), Some(&exe_dir))
            .expect("plan builds");

        assert_eq!(plan.dbus, DbusAction::UseExisting);
    }

    #[test]
    fn exported_environment_contains_session_values_but_not_kept_hints() {
        let scratch = Scratch::new("plan-env");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");
        let env = Environment::new([
            pair("PATH", ""),
            pair("XDG_RUNTIME_DIR", "/run/user/1000"),
            pair("GDK_BACKEND", "x11"),
        ]);

        let plan = build_plan(&PlanRequest::default(), &env, Some(&exe_dir)).expect("plan builds");
        let specs = plan.component_specs();

        assert!(specs[0].env.contains(&pair("XDG_SESSION_TYPE", "wayland")));
        assert!(
            specs[0]
                .env
                .contains(&pair("QT_QPA_PLATFORM", "wayland;xcb"))
        );
        assert!(!specs[0].env.iter().any(|(key, _)| key == "GDK_BACKEND"));
        assert_eq!(specs[1].wayland, WaylandRole::Client);
    }

    #[test]
    fn runtime_dir_decides_whether_readiness_is_configured() {
        let scratch = Scratch::new("plan-runtime");
        install_all(&scratch);
        let exe_dir = scratch.path().join("bin");
        let without = Environment::new([pair("PATH", "")]);

        let plan =
            build_plan(&PlanRequest::default(), &without, Some(&exe_dir)).expect("plan builds");

        assert_eq!(plan.readiness(), None);
        let plan = build_plan(&PlanRequest::default(), &base_env(&[]), Some(&exe_dir))
            .expect("plan builds");
        assert_eq!(
            plan.readiness().map(|r| r.runtime_dir),
            Some(PathBuf::from("/run/user/1000"))
        );
    }
}
