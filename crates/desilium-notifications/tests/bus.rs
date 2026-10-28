// Test code: a failing unwrap or expect is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! End-to-end test over a private session bus.
//!
//! Skips (passes with a message) when `dbus-daemon` is not installed.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread;
use std::time::Duration;

use async_io::Timer;
use desilium_notifications::dbus::{
    FREEDESKTOP_PATH, FREEDESKTOP_SERVICE, SHELL_PATH, SHELL_SERVICE, Service, WireNotification,
};
use futures_lite::{Stream, StreamExt, future};
use zbus::zvariant::Value;
use zbus::{Proxy, connection::Builder};

const SIGNAL_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSED_BY_CALL: u32 = 3;
const CLOSED_EXPIRED: u32 = 1;

/// A `dbus-daemon --session` child that is killed when the test ends.
struct PrivateBus {
    child: Child,
    _stdout: BufReader<ChildStdout>,
    address: String,
}

impl PrivateBus {
    fn start() -> Option<Self> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdout = BufReader::new(child.stdout.take()?);
        let mut line = String::new();
        stdout.read_line(&mut line).ok()?;
        let address = line.trim().to_string();
        if address.is_empty() {
            let _ = child.kill();
            return None;
        }
        Some(Self {
            child,
            _stdout: stdout,
            address,
        })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn next_within<S>(stream: &mut S) -> Option<S::Item>
where
    S: Stream + Unpin,
{
    future::or(async { stream.next().await }, async {
        Timer::after(SIGNAL_TIMEOUT).await;
        None
    })
    .await
}

async fn expect_signal(stream: &mut (impl Stream<Item = zbus::Message> + Unpin)) -> zbus::Message {
    next_within(stream)
        .await
        .expect("a signal arrives within the timeout")
}

/// Skips signals from earlier steps until the one carrying `expected` arrives.
async fn added_id(stream: &mut (impl Stream<Item = zbus::Message> + Unpin), expected: u32) {
    for _ in 0..50 {
        let message = expect_signal(stream).await;
        if message.body().deserialize::<u32>().expect("id") == expected {
            return;
        }
    }
    panic!("Added({expected}) never arrived");
}

async fn notify(
    client: &Proxy<'_>,
    replaces: u32,
    summary: &str,
    expire_ms: i32,
    actions: Vec<String>,
) -> u32 {
    let hints: HashMap<&str, Value<'_>> = HashMap::new();
    client
        .call::<_, _, u32>(
            "Notify",
            &(
                "TestApp",
                replaces,
                "",
                summary,
                "<b>body</b>",
                actions,
                hints,
                expire_ms,
            ),
        )
        .await
        .expect("Notify succeeds")
}

async fn active_toasts(shell: &Proxy<'_>) -> Vec<WireNotification> {
    shell
        .call::<_, _, Vec<WireNotification>>("ActiveToasts", &())
        .await
        .expect("ActiveToasts succeeds")
}

async fn wait_until_served(client: &Proxy<'_>) {
    for _ in 0..100 {
        let answer = client
            .call::<_, _, (String, String, String, String)>("GetServerInformation", &())
            .await;
        if answer.is_ok() {
            return;
        }
        Timer::after(Duration::from_millis(50)).await;
    }
    panic!("the service never registered its name");
}

async fn scenario(address: &str) {
    let connection = Builder::address(address)
        .expect("valid address")
        .build()
        .await
        .expect("client connects");
    let fd = Proxy::new(
        &connection,
        FREEDESKTOP_SERVICE,
        FREEDESKTOP_PATH,
        FREEDESKTOP_SERVICE,
    )
    .await
    .expect("freedesktop proxy");
    let shell = Proxy::new(&connection, SHELL_SERVICE, SHELL_PATH, SHELL_SERVICE)
        .await
        .expect("shell proxy");
    wait_until_served(&fd).await;

    let mut closed = fd
        .receive_signal("NotificationClosed")
        .await
        .expect("subscribe");
    let mut invoked = fd.receive_signal("ActionInvoked").await.expect("subscribe");
    let mut shell_added = shell.receive_signal("Added").await.expect("subscribe");
    let mut shell_dnd = shell.receive_signal("DndChanged").await.expect("subscribe");

    // Server information and capabilities.
    let info: (String, String, String, String) = fd
        .call("GetServerInformation", &())
        .await
        .expect("GetServerInformation");
    assert_eq!(info.0, "Desilium");
    assert_eq!(info.1, "Myronide");
    assert_eq!(info.3, "1.2");
    let caps: Vec<String> = fd.call("GetCapabilities", &()).await.expect("capabilities");
    assert!(caps.iter().any(|cap| cap == "body-markup"));
    assert!(caps.iter().any(|cap| cap == "actions"));

    // Replace keeps the id; CloseNotification emits NotificationClosed with reason 3.
    let first = notify(&fd, 0, "first", 0, Vec::new()).await;
    let replaced = notify(&fd, first, "second", 0, Vec::new()).await;
    assert_eq!(replaced, first, "replaces_id keeps the id");
    fd.call_noreply("CloseNotification", &(first,))
        .await
        .expect("CloseNotification");
    let message = expect_signal(&mut closed).await;
    let (closed_id, reason) = message
        .body()
        .deserialize::<(u32, u32)>()
        .expect("NotificationClosed body");
    assert_eq!((closed_id, reason), (first, CLOSED_BY_CALL));

    // A short toast expires with reason 1.
    let expiring = notify(&fd, 0, "gone soon", 150, Vec::new()).await;
    let message = expect_signal(&mut closed).await;
    let (closed_id, reason) = message
        .body()
        .deserialize::<(u32, u32)>()
        .expect("NotificationClosed body");
    assert_eq!((closed_id, reason), (expiring, CLOSED_EXPIRED));

    // Do-not-disturb: the property flips, the signal fires, and normal toasts stop.
    connection
        .call_method(
            Some(SHELL_SERVICE),
            SHELL_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(SHELL_SERVICE, "DoNotDisturb", Value::from(true)),
        )
        .await
        .expect("set DoNotDisturb");
    let dnd_on = shell
        .get_property::<bool>("DoNotDisturb")
        .await
        .expect("read DoNotDisturb");
    assert!(dnd_on);
    let message = expect_signal(&mut shell_dnd).await;
    assert!(message.body().deserialize::<bool>().expect("bool"));

    let quiet = notify(&fd, 0, "quiet", 0, Vec::new()).await;
    added_id(&mut shell_added, quiet).await;
    assert!(
        active_toasts(&shell)
            .await
            .iter()
            .all(|toast| toast.id != quiet),
        "no toast while do-not-disturb is on"
    );

    // A critical notification still gets a toast during do-not-disturb.
    let hints: HashMap<&str, Value<'_>> = HashMap::from([("urgency", Value::from(2u8))]);
    let alarm: u32 = fd
        .call(
            "Notify",
            &(
                "TestApp",
                0u32,
                "",
                "alarm",
                "",
                Vec::<String>::new(),
                hints,
                0i32,
            ),
        )
        .await
        .expect("critical Notify");
    assert!(
        active_toasts(&shell)
            .await
            .iter()
            .any(|toast| toast.id == alarm)
    );

    // Actions: InvokeAction emits ActionInvoked and closes the entry.
    let actionable = notify(&fd, 0, "pick", 0, vec!["default".into(), "Open".into()]).await;
    added_id(&mut shell_added, actionable).await;
    shell
        .call_noreply("InvokeAction", &(actionable, "default"))
        .await
        .expect("InvokeAction");
    let message = expect_signal(&mut invoked).await;
    let (invoked_id, key) = message
        .body()
        .deserialize::<(u32, String)>()
        .expect("ActionInvoked body");
    assert_eq!((invoked_id, key.as_str()), (actionable, "default"));

    // The history is listed, and DismissAll empties it.
    let history: Vec<WireNotification> = shell.call("List", &()).await.expect("List");
    assert!(history.iter().any(|item| item.id == quiet));
    assert!(history.iter().all(|item| item.id != actionable));
    shell
        .call_noreply("DismissAll", &())
        .await
        .expect("DismissAll");
    let history: Vec<WireNotification> = shell.call("List", &()).await.expect("List");
    assert!(history.is_empty());
    assert!(active_toasts(&shell).await.is_empty());

    connection
        .call_method(
            Some(SHELL_SERVICE),
            SHELL_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(SHELL_SERVICE, "DoNotDisturb", Value::from(false)),
        )
        .await
        .expect("clear DoNotDisturb");
    let message = expect_signal(&mut shell_dnd).await;
    assert!(!message.body().deserialize::<bool>().expect("bool"));
}

#[test]
fn freedesktop_and_shell_interfaces_work_over_a_private_bus() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("skipping: dbus-daemon is not available");
        return;
    };
    let service_address = bus.address.clone();
    thread::spawn(move || {
        async_io::block_on(async move {
            match Service::connect(Some(&service_address)).await {
                Ok(service) => service.run().await,
                Err(error) => eprintln!("service failed to start: {error}"),
            }
        });
    });
    async_io::block_on(scenario(&bus.address));
}
