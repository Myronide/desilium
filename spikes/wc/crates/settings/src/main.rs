use dioxus::prelude::*;
use wc_core::{FocusPolicy, Request, Settings};

const CSS: &str = include_str!("style.css");

fn main() {
    if std::env::args().any(|a| a == "--help") {
        println!(
            "wc-settings — native Dioxus/Blitz-Einstellungen für WC\nWC_SOCKET wählt die Compositor-Instanz. Ohne laufende Instanz ist explizites Offline-Speichern möglich."
        );
        return;
    }
    let attributes = winit::window::Window::default_attributes()
        .with_title("WC · Einstellungen")
        .with_inner_size(winit::dpi::LogicalSize::new(1000.0, 680.0))
        .with_min_inner_size(winit::dpi::LogicalSize::new(840.0, 600.0));
    dioxus_native::launch_cfg(app, vec![], vec![Box::new(attributes)]);
}
async fn call(request: Request) -> Result<wc_core::Snapshot, String> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(wc_core::request(&request));
    });
    rx.recv().await.map_err(|e| e.to_string())?
}
fn app() -> Element {
    let mut draft = use_signal(Settings::default);
    let mut saved = use_signal(Settings::default);
    let mut page = use_signal(|| "Fenster".to_string());
    let mut message = use_signal(|| "Verbindung wird hergestellt …".to_string());
    let mut connected = use_signal(|| false);
    let mut busy = use_signal(|| true);
    let mut windows = use_signal(|| 0usize);
    let mut display = use_signal(String::new);
    let mut query = use_signal(String::new);
    use_future(move || async move {
        match call(Request::Get).await {
            Ok(snapshot) => {
                draft.set(snapshot.settings.clone());
                saved.set(snapshot.settings);
                windows.set(snapshot.windows);
                display.set(snapshot.wayland_display);
                connected.set(true);
                message.set("Verbunden. Änderungen mit „Übernehmen“ aktivieren.".into());
            }
            Err(error) => {
                message.set(match wc_core::load() {
                    Ok(settings) => { draft.set(settings.clone()); saved.set(settings); format!("Offline · {error}") }
                    Err(e) => format!("Konfiguration konnte nicht geladen werden: {e}. Datei bleibt bis zum expliziten Speichern unverändert."),
                });
            }
        }
        busy.set(false);
    });
    let changed = draft() != saved();
    let validation = draft().validate().err();
    let accent = draft().background;
    let gap = draft().gap.min(32);
    let search = query().to_lowercase();
    let show = |keywords: &str| search.is_empty() || keywords.to_lowercase().contains(&search);
    rsx! {
        style { {CSS} }
        div { class: "app",
            aside { class: "sidebar",
                div { class: "brand", span { class: "brand-icon", "W" } div { strong { "WC" } small { "WAYLAND COMPOSITOR" } } }
                div { class: "nav-caption", "DEIN DESKTOP" }
                for (name, icon) in [("Fenster", "▣"), ("Tastatur", "⌨"), ("Design", "◈"), ("Shortcuts", "⌘")] {
                    button {
                        class: if page() == name { "nav selected" } else { "nav" },
                        onclick: move |_| { page.set(name.into()); query.set(String::new()); },
                        span { "{icon}" } "{name}"
                    }
                }
                div { class: "sidebar-bottom",
                    div { class: "status-dot", if connected() { "● VERBUNDEN" } else { "○ OFFLINE" } }
                    p { "Floating first. Dein Workflow." }
                    small { "Rust + Smithay + Blitz" }
                }
            }
            main { class: "main",
                header {
                    div { class: "eyebrow", "ARBEITSUMGEBUNG / EINSTELLUNGEN" }
                    h1 { "{page}" }
                    p { "Ein Desktop, der sich nach dir richtet." }
                }
                div { class: "toolbar",
                    input { class: "search", placeholder: "Auf dieser Seite suchen …", value: "{query}", oninput: move |e| query.set(e.value()), aria_label: "Einstellungen suchen" }
                    span { class: "badge", "FLOATING FIRST" }
                }
                div { class: "content",
                    if page() == "Fenster" {
                        if show("fokus maus klick anheben fenster") {
                            section { class: "card",
                                h2 { "Fokus & Fenster" }
                                p { class: "description", "Du entscheidest, welches Fenster im Mittelpunkt steht." }
                                div { class: "row",
                                    div { strong { "Fokusmodus" } small { "Klick oder Mauszeiger aktiviert ein Fenster." } }
                                    div { class: "segmented",
                                        button { class: if draft().focus == FocusPolicy::Click { "active" } else { "" }, onclick: move |_| draft.write().focus = FocusPolicy::Click, "Klick" }
                                        button { class: if draft().focus == FocusPolicy::Sloppy { "active" } else { "" }, onclick: move |_| draft.write().focus = FocusPolicy::Sloppy, "Maus" }
                                    }
                                }
                                Toggle { title: "Bei Mausfokus anheben", detail: "Im Mausmodus kommt das fokussierte Fenster nach vorn.", value: draft().raise_on_focus, onchange: move |v| draft.write().raise_on_focus = v }
                            }
                        }
                        if show("snap einrasten rand abstand fenster maximieren") {
                            section { class: "card",
                                h2 { "Einrasten & Anordnen" }
                                p { class: "description", "An den Seiten halbieren, am oberen Rand maximieren." }
                                Toggle { title: "An Bildschirmrändern einrasten", detail: "Fenster an den Rand ziehen und loslassen.", value: draft().snap_enabled, onchange: move |v| draft.write().snap_enabled = v }
                                Number { title: "Einrastabstand", detail: "Entfernung zum Rand in Pixeln.", value: draft().snap_distance, min: 4, max: 96, step: 4, unit: "px", onchange: move |v| draft.write().snap_distance = v }
                                Number { title: "Fensterabstand", detail: "Rand und Abstand zwischen eingerasteten Fenstern.", value: draft().gap, min: 0, max: 64, step: 2, unit: "px", onchange: move |v| draft.write().gap = v }
                            }
                        }
                    }
                    if page() == "Tastatur" {
                        if show("tastatur wiederholung rate verzögerung terminal programm") {
                            section { class: "card",
                                h2 { "Eingabe" }
                                Number { title: "Wiederholungsverzögerung", detail: "Wartezeit bis zum Wiederholen einer gehaltenen Taste.", value: draft().keyboard_repeat_delay, min: 100, max: 1500, step: 50, unit: "ms", onchange: move |v| draft.write().keyboard_repeat_delay = v }
                                Number { title: "Wiederholungsrate", detail: "Zeichen pro Sekunde bei gehaltener Taste.", value: draft().keyboard_repeat_rate, min: 1, max: 100, step: 1, unit: "Hz", onchange: move |v| draft.write().keyboard_repeat_rate = v }
                                TextField { title: "Terminal", detail: "Programmname oder absoluter Pfad, ohne Argumente.", value: draft().terminal, onchange: move |v| draft.write().terminal = v }
                            }
                        }
                    }
                    if page() == "Design" {
                        if show("hintergrund farbe design vorschau abstand") {
                            section { class: "card",
                                h2 { "Deine Arbeitsfläche" }
                                p { class: "description", "Die Vorschau zeigt die Hintergrundfarbe und Fensterabstände." }
                                div { class: "preview", style: "background: {accent}; padding: {gap}px; gap: {gap}px;",
                                    div { class: "mock-window", div { class: "mock-title", "Dateien" } div { class: "mock-line" } div { class: "mock-line short" } }
                                    div { class: "mock-window", div { class: "mock-title", "Terminal" } div { class: "mock-code", "$ cargo run" } }
                                }
                                div { class: "swatches",
                                    for color in ["#101827", "#171923", "#132c29", "#29223d", "#2e2025", "#dde5ed"] {
                                        button { style: "background: {color}", aria_label: "Hintergrund {color}", onclick: move |_| draft.write().background = color.into(), if draft().background == color { "✓" } }
                                    }
                                }
                                TextField { title: "Eigene Hintergrundfarbe", detail: "Hex-Farbe im Format #RRGGBB.", value: draft().background, onchange: move |v| draft.write().background = v }
                            }
                        }
                    }
                    if page() == "Shortcuts" {
                        section { class: "card",
                            h2 { "Deine Tastenkürzel" }
                            p { class: "description", "Super, Ctrl, Alt, Shift + Taste. Tastennamen: Return, comma, Left, F1 … F12. Jedes Kürzel muss eindeutig sein." }
                            for (id, label, value) in shortcut_fields(&draft()) {
                                if show(&format!("{label} {value}")) {
                                    TextField { key: "{id}", title: label, detail: "", value, onchange: move |v| set_shortcut(&mut draft.write(), id, v) }
                                }
                            }
                        }
                    }
                    if !search.is_empty() { p { class: "search-hint", "Filter: {search} · Leeres Suchfeld zeigt alle Optionen dieser Seite." } }
                    section { class: "info",
                        strong { if connected() { "Instanz: {display} · {windows} Fenster bei letzter Abfrage" } else { "Offline-Konfiguration" } }
                        p { "Einstellungen wirken ohne Neustart. Maximierte und eingerastete Fenster werden neu angeordnet." }
                        button { class: "text-button", disabled: busy(), onclick: move |_| {
                            busy.set(true);
                            spawn(async move {
                                match call(Request::Get).await {
                                    Ok(s) => { connected.set(true); windows.set(s.windows); display.set(s.wayland_display); message.set("Verbunden. Lokale Änderungen wurden beibehalten.".into()); }
                                    Err(e) => { connected.set(false); message.set(e); }
                                }
                                busy.set(false);
                            });
                        }, "Verbindung prüfen" }
                    }
                }
                footer {
                    div { class: "feedback",
                        span { class: if validation.is_some() { "error" } else { "" },
                            if let Some(error) = validation.clone() { "{error}" } else { "{message}" }
                        }
                        small { if changed { "Ungespeicherte Änderungen" } else { "Keine ausstehenden Änderungen" } }
                    }
                    div { class: "footer-actions",
                        button { disabled: busy() || !changed, onclick: move |_| draft.set(saved()), "Verwerfen" }
                        button { disabled: busy(), onclick: move |_| { draft.set(Settings::default()); message.set("Standardwerte als Entwurf geladen.".into()); }, "Standardwerte" }
                        if connected() {
                            button { class: "primary", disabled: busy() || !changed || validation.is_some(), onclick: move |_| {
                                let settings = draft(); busy.set(true);
                                spawn(async move {
                                    match call(Request::Apply(Box::new(settings))).await {
                                        Ok(s) => { saved.set(s.settings); windows.set(s.windows); message.set("Gespeichert und angewendet.".into()); }
                                        Err(e) => message.set(format!("Nicht angewendet: {e}")),
                                    }
                                    busy.set(false);
                                });
                            }, if busy() { "Übernimmt …" } else { "Übernehmen" } }
                        } else {
                            button { class: "primary", disabled: busy() || !changed || validation.is_some(), onclick: move |_| {
                                match wc_core::save(&draft()) {
                                    Ok(()) => { saved.set(draft()); message.set("Offline gespeichert. Wird beim nächsten Compositor-Start geladen.".into()); }
                                    Err(e) => message.set(format!("Speichern fehlgeschlagen: {e}")),
                                }
                            }, "Offline speichern" }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn Toggle(title: String, detail: String, value: bool, onchange: EventHandler<bool>) -> Element {
    rsx! { div { class: "row", div { strong { "{title}" } small { "{detail}" } }
        button { class: if value { "toggle on" } else { "toggle" }, role: "switch", aria_checked: "{value}", aria_label: "{title}", onclick: move |_| onchange.call(!value), span { if value { "An" } else { "Aus" } } }
    } }
}
#[component]
fn Number(
    title: String,
    detail: String,
    value: u16,
    min: u16,
    max: u16,
    step: u16,
    unit: String,
    onchange: EventHandler<u16>,
) -> Element {
    rsx! { div { class: "row", div { strong { "{title}" } small { "{detail}" } }
        div { class: "stepper",
            button { disabled: value <= min, aria_label: "{title} verringern", onclick: move |_| onchange.call(value.saturating_sub(step).max(min)), "−" }
            span { "{value} {unit}" }
            button { disabled: value >= max, aria_label: "{title} erhöhen", onclick: move |_| onchange.call(value.saturating_add(step).min(max)), "+" }
        }
    } }
}
#[component]
fn TextField(
    title: String,
    detail: String,
    value: String,
    onchange: EventHandler<String>,
) -> Element {
    rsx! { div { class: "row", div { strong { "{title}" } small { "{detail}" } }
        input { class: "field", aria_label: "{title}", value, oninput: move |e| onchange.call(e.value()) }
    } }
}
fn shortcut_fields(s: &Settings) -> Vec<(&'static str, String, String)> {
    [
        ("terminal", "Terminal öffnen", &s.shortcuts.terminal),
        ("settings", "Einstellungen öffnen", &s.shortcuts.settings),
        ("close", "Fenster schließen", &s.shortcuts.close),
        ("cycle", "Fenster wechseln", &s.shortcuts.cycle),
        ("maximize", "Maximieren", &s.shortcuts.maximize),
        ("restore", "Wiederherstellen", &s.shortcuts.restore),
        ("left", "Linke Hälfte", &s.shortcuts.left),
        ("right", "Rechte Hälfte", &s.shortcuts.right),
        ("quit", "Compositor beenden", &s.shortcuts.quit),
    ]
    .into_iter()
    .map(|(id, label, value)| (id, label.into(), value.clone()))
    .collect()
}
fn set_shortcut(s: &mut Settings, id: &str, v: String) {
    match id {
        "terminal" => s.shortcuts.terminal = v,
        "settings" => s.shortcuts.settings = v,
        "close" => s.shortcuts.close = v,
        "cycle" => s.shortcuts.cycle = v,
        "maximize" => s.shortcuts.maximize = v,
        "restore" => s.shortcuts.restore = v,
        "left" => s.shortcuts.left = v,
        "right" => s.shortcuts.right = v,
        "quit" => s.shortcuts.quit = v,
        _ => {}
    }
}
