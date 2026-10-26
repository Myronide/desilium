//! Human-readable, JSON and shell-export views of a `SessionPlan`.

use std::fmt::Write as _;

use serde_json::{Value, json};

use crate::startup::dbus::DbusAction;
use crate::startup::environment::{EnvChange, EnvEntry};
use crate::startup::plan::SessionPlan;

/// Readable plan for `--dry-run`.
pub fn render_text(plan: &SessionPlan) -> String {
    let mut out = String::from("desilium session plan\n");
    let _ = writeln!(out, "  dbus: {}", dbus_text(&plan.dbus));
    let runtime = plan.runtime_dir.as_ref().map_or_else(
        || "not set (a real start fails without XDG_RUNTIME_DIR)".to_owned(),
        |dir| dir.display().to_string(),
    );
    let _ = writeln!(out, "  runtime dir: {runtime}");
    out.push_str("  start order:\n");
    for (index, component) in plan.components.iter().enumerate() {
        let _ = writeln!(
            out,
            "    {}. {} [{}, wayland {}] {} ({})",
            index + 1,
            component.name,
            component.criticality.label(),
            component.wayland.label(),
            component.program.display(),
            component.found_via.label(),
        );
        if !component.args.is_empty() {
            let args: Vec<String> = component
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            let _ = writeln!(out, "       args: {}", args.join(" "));
        }
    }
    out.push_str("  skipped:\n");
    if plan.skipped.is_empty() {
        out.push_str("    (none)\n");
    }
    for skipped in &plan.skipped {
        let _ = writeln!(
            out,
            "    - {}: {} not installed",
            skipped.name, skipped.binary
        );
    }
    out.push_str("  environment:\n");
    for entry in &plan.env {
        let _ = writeln!(out, "    {}", env_line(entry));
    }
    out.push_str(
        "  WAYLAND_DISPLAY: set by the supervisor once the compositor creates its socket\n",
    );
    out
}

fn dbus_text(action: &DbusAction) -> String {
    match action {
        DbusAction::UseExisting => "use the running session bus".to_owned(),
        DbusAction::ReexecUnder { launcher } => {
            format!("re-exec under {}", launcher.display())
        }
        DbusAction::Unavailable => {
            "no session bus and no dbus-run-session; continuing without".to_owned()
        }
    }
}

fn env_line(entry: &EnvEntry) -> String {
    let value = entry.value().to_string_lossy();
    match &entry.change {
        EnvChange::Kept { .. } => format!("= {}={value} (kept: set by the user)", entry.key),
        EnvChange::Set { previous: None, .. } => format!("+ {}={value}", entry.key),
        EnvChange::Set {
            previous: Some(previous),
            ..
        } if previous == entry.value() => format!("= {}={value} (unchanged)", entry.key),
        EnvChange::Set {
            previous: Some(previous),
            ..
        } => format!(
            "~ {}={value} (was {})",
            entry.key,
            previous.to_string_lossy()
        ),
    }
}

/// Machine-readable plan for `--dry-run=json`.
pub fn render_json(plan: &SessionPlan) -> String {
    let components: Vec<Value> = plan
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            json!({
                "order": index + 1,
                "name": component.name,
                "program": component.program.display().to_string(),
                "found_via": component.found_via.label(),
                "criticality": component.criticality.label(),
                "wayland": component.wayland.label(),
                "args": component.args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let skipped: Vec<Value> = plan
        .skipped
        .iter()
        .map(|item| json!({ "name": item.name, "binary": item.binary }))
        .collect();
    let environment: Vec<Value> = plan.env.iter().map(env_json).collect();
    let value = json!({
        "dbus": dbus_json(&plan.dbus),
        "runtime_dir": plan.runtime_dir.as_ref().map(|dir| dir.display().to_string()),
        "components": components,
        "skipped": skipped,
        "environment": environment,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn dbus_json(action: &DbusAction) -> Value {
    match action {
        DbusAction::ReexecUnder { launcher } => json!({
            "action": action.label(),
            "launcher": launcher.display().to_string(),
        }),
        _ => json!({ "action": action.label() }),
    }
}

fn env_json(entry: &EnvEntry) -> Value {
    let value = entry.value().to_string_lossy().into_owned();
    match &entry.change {
        EnvChange::Kept { .. } => json!({ "key": entry.key, "value": value, "action": "kept" }),
        EnvChange::Set { previous, .. } => json!({
            "key": entry.key,
            "value": value,
            "action": "set",
            "previous": previous.as_ref().map(|old| old.to_string_lossy().into_owned()),
        }),
    }
}

/// Shell `export` lines for `--print-env`.
pub fn render_exports(plan: &SessionPlan) -> String {
    let mut out = String::from(
        "# Desilium session environment. WAYLAND_DISPLAY is set by the supervisor at runtime.\n",
    );
    for entry in &plan.env {
        let value = entry.value().to_string_lossy();
        match entry.change {
            EnvChange::Set { .. } => {
                let _ = writeln!(out, "export {}={}", entry.key, shell_quote(&value));
            }
            EnvChange::Kept { .. } => {
                let _ = writeln!(out, "# kept (set by the user): {}={}", entry.key, value);
            }
        }
    }
    out
}

/// Single-quotes `value` for POSIX shells.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{Criticality, WaylandRole};
    use crate::startup::defaults::FoundVia;
    use crate::startup::environment::Environment;
    use crate::startup::plan::{PlannedComponent, SessionPlan};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn sample_plan() -> SessionPlan {
        let env = Environment::new([
            (OsString::from("XDG_SESSION_TYPE"), OsString::from("x11")),
            (OsString::from("GDK_BACKEND"), OsString::from("x11")),
        ]);
        SessionPlan {
            components: vec![PlannedComponent {
                name: "compositor",
                program: PathBuf::from("/opt/desilium/bin/desilium-compositor"),
                found_via: FoundVia::Sibling,
                criticality: Criticality::Critical,
                wayland: WaylandRole::Provider,
                args: vec![OsString::from("--nested")],
            }],
            skipped: vec![crate::startup::plan::SkippedComponent {
                name: "shell",
                binary: "desilium-shell",
            }],
            dbus: DbusAction::UseExisting,
            env: crate::startup::environment::session_environment(&env),
            runtime_dir: Some(PathBuf::from("/run/user/1000")),
        }
    }

    #[test]
    fn text_plan_lists_order_skips_and_env_changes() {
        let text = render_text(&sample_plan());
        assert!(text.contains("1. compositor [critical, wayland provider]"));
        assert!(text.contains("args: --nested"));
        assert!(text.contains("- shell: desilium-shell not installed"));
        assert!(text.contains("~ XDG_SESSION_TYPE=wayland (was x11)"));
        assert!(text.contains("= GDK_BACKEND=x11 (kept: set by the user)"));
        assert!(text.contains("+ QT_QPA_PLATFORM=wayland;xcb"));
    }

    #[test]
    fn json_plan_is_valid_and_carries_the_same_facts() {
        let value: Value = serde_json::from_str(&render_json(&sample_plan())).expect("valid JSON");
        assert_eq!(value["dbus"]["action"], "use_existing");
        assert_eq!(value["runtime_dir"], "/run/user/1000");
        assert_eq!(value["components"][0]["name"], "compositor");
        assert_eq!(value["components"][0]["wayland"], "provider");
        assert_eq!(value["skipped"][0]["binary"], "desilium-shell");
        let session = value["environment"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["key"] == "XDG_SESSION_TYPE"))
            .expect("session variable present");
        assert_eq!(session["previous"], "x11");
    }

    #[test]
    fn exports_quote_values_and_comment_kept_hints() {
        let exports = render_exports(&sample_plan());
        assert!(exports.contains("export XDG_SESSION_TYPE='wayland'"));
        assert!(exports.contains("# kept (set by the user): GDK_BACKEND=x11"));
        assert!(!exports.contains("export GDK_BACKEND"));
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote("wayland;xcb"), "'wayland;xcb'");
    }
}
