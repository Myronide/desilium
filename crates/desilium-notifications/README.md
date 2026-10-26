# desilium-notifications

Desilium's notification service. It is a daemon that owns two session-bus names:

- `org.freedesktop.Notifications` (spec 1.2), which applications use.
- `org.desilium.Notifications1`, which only the shell uses to render toasts and the notification center.

The state lives in a pure model (`src/model/`, no D-Bus types). `src/dbus/` is a thin adapter.

## Interfaces

### `org.freedesktop.Notifications` at `/org/freedesktop/Notifications`

| Member | Kind | Notes |
|---|---|---|
| `GetCapabilities() -> as` | method | `actions`, `body`, `body-markup`, `body-hyperlinks`, `icon-static`, `persistence` |
| `Notify(s app_name, u replaces_id, s app_icon, s summary, s body, as actions, a{sv} hints, i expire_timeout) -> u` | method | Hints: `urgency`, `desktop-entry`, `image-path`/`image_path`, `image-data`, `category`, `transient`, `resident`, `value`, `x-canonical-private-synchronous` |
| `CloseNotification(u id)` | method | Emits `NotificationClosed(id, 3)`. Unknown ids are ignored. |
| `GetServerInformation() -> (s, s, s, s)` | method | `Desilium`, `Myronide`, crate version, `1.2` |
| `NotificationClosed(u id, u reason)` | signal | Reasons 1 expired, 2 dismissed, 3 closed by call, 4 undefined |
| `ActionInvoked(u id, s action_key)` | signal | |

Expiry: `-1` uses the urgency default (low 5 s, normal 8 s). `0` never expires. Critical notifications never expire on their own. Explicit values are capped at 10 minutes.

### `org.desilium.Notifications1` at `/org/desilium/Notifications1`

| Member | Kind | Notes |
|---|---|---|
| `DoNotDisturb` | property (read/write, `b`) | Non-critical notifications go to the history without a toast. |
| `List() -> a(WireNotification)` | method | History, newest first. |
| `ListGroups() -> a(WireGroup)` | method | History grouped by application, most recently active first. |
| `ActiveToasts() -> a(WireNotification)` | method | The toast stack, newest first, at most 3. The timers run in the daemon, so the shell only renders this. |
| `Dismiss(u id)`, `DismissAll()` | methods | Remove from the history. |
| `InvokeAction(u id, s key) -> ()` | method | Errors on an unknown id or action. Non-resident entries are closed afterwards. |
| `MarkRead(u id)` | method | |
| `IconPixels(u id) -> (u, u, ay)` | method | RGBA pixels for an entry whose `icon` is `rgba`. |
| `Added`, `Updated`, `Removed` (`u id`) | signals | `Removed` also means the freedesktop client receives `NotificationClosed`. |
| `ToastClosed(u id)` | signal | The toast expired. The entry stays in the history. |
| `ToastHidden(u id)` | signal | Toast demoted by the stack limit or DND. The entry stays in the history. |
| `DndChanged(b enabled)` | signal | |

`WireNotification` fields: `id`, `app_key`, `app_name`, `summary`, `body` (spans: text, bold, italic, underline, link), `icon` (`none`, `name:<n>`, `path:<p>`, `rgba`), `actions` (key, label), `urgency` (0-2), `received_unix_ms`, `progress` (-1 if none), `read`, `toast`.

The shell should re-query `ActiveToasts` after any of `Added`, `Updated`, `Removed`, `ToastClosed`, `ToastHidden` or `DndChanged`.

## Limits

Summary 256 characters, body 4096 characters, 8 action pairs, 64 characters per action label, 32 hints, 512 px per image edge and 1 MiB per image. The history holds 200 entries, dropping the oldest non-critical one first. Each application gets a token bucket of 8 notifications, refilled at one per 500 ms.

Body markup keeps only `<b>`, `<i>`, `<u>`, `<a href>` (http, https and mailto only) and `<br>`. All other tags, including `<img>`, are dropped and their text is kept. Control characters are removed.

## Running

```sh
cargo run -p desilium-notifications            # uses the session bus
RUST_LOG=debug cargo run -p desilium-notifications
```

The daemon needs the `org.freedesktop.Notifications` name, so stop any other notification daemon first (mako, dunst, swaync, the desktop shell's own daemon). Otherwise the name is taken and the daemon exits with an error.

### Manual testing

```sh
# Toasts and history
notify-send -a Mail -u critical "Build failed" "<b>main</b> &amp; <a href='https://example.org'>details</a>"
notify-send -a Chat -t 0 "Sticky" "does not expire"

# Shell-side view
busctl --user call org.desilium.Notifications1 /org/desilium/Notifications1 org.desilium.Notifications1 ActiveToasts
busctl --user call org.desilium.Notifications1 /org/desilium/Notifications1 org.desilium.Notifications1 ListGroups
busctl --user get-property org.desilium.Notifications1 /org/desilium/Notifications1 org.desilium.Notifications1 DoNotDisturb
busctl --user set-property org.desilium.Notifications1 /org/desilium/Notifications1 org.desilium.Notifications1 DoNotDisturb b true
busctl --user call org.desilium.Notifications1 /org/desilium/Notifications1 org.desilium.Notifications1 DismissAll

# Watch signals while exercising the above
busctl --user monitor org.desilium.Notifications1
busctl --user monitor org.freedesktop.Notifications
```

To test on a private bus so the desktop's own daemon is untouched:

```sh
ADDR=$(dbus-daemon --session --print-address=1 --fork --print-pid=1 | head -1)
DBUS_SESSION_BUS_ADDRESS="$ADDR" cargo run -p desilium-notifications &
DBUS_SESSION_BUS_ADDRESS="$ADDR" notify-send "hello" "from the private bus"
```

## Tests

```sh
cargo test -p desilium-notifications
```

The unit tests cover the model, markup, hints, image validation, policy and rate limiting, with time passed in explicitly. `tests/bus.rs` starts a private `dbus-daemon --session` and drives both interfaces with a zbus client. It passes without running anything when `dbus-daemon` is not installed.

## Design notes

- **Backend.** zbus 5 with its default `async-io` backend. The daemon has no other async work, so it needs no tokio runtime. The timer task is a single `async_io::Timer` that sleeps until the next toast deadline or until a request wakes it, so idle CPU use is near zero.
- **Time.** The model takes `Timestamp` arguments. The daemon reads the real clock in one place.
