mod dialog;

use crate::{
    app::App,
    model::{Entry, Sort},
};

pub const CSS: &str = include_str!("style.css");

pub fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn icon(name: &str) -> String {
    desilium_ui::icon_svg(name, 16, "")
}

fn button(action: &str, label: &str) -> String {
    format!(
        "<button data-action='{}'>{}</button>",
        esc(action),
        esc(label)
    )
}

pub fn document(app: &App) -> String {
    format!("<div id=root>{}</div>", html(app))
}

pub fn html(app: &App) -> String {
    format!(
        "<div class=files><aside>{}</aside><main>{}<section id=listing>{}</section><footer id=status>{}</footer></main></div>{}",
        sidebar(app),
        toolbar(app),
        listing(app),
        status(app),
        dialog::html(app)
    )
}

/// Folders of the home directory shown under Places, with their icon.
const HOME_FOLDERS: [(&str, &str); 6] = [
    ("Desktop", "monitor"),
    ("Documents", "file"),
    ("Downloads", "enter"),
    ("Pictures", "image"),
    ("Music", "music"),
    ("Videos", "play"),
];
fn sidebar(app: &App) -> String {
    let current = app.browser.current.0.as_str();
    let mut html = String::from("<div class=app-title>Doria</div><nav>");
    html.push_str(&place(
        current,
        &app.home.display().to_string(),
        "Home",
        "folder",
    ));
    for (folder, glyph) in HOME_FOLDERS {
        let path = app.home.join(folder);
        if path.is_dir() {
            html.push_str(&place(current, &path.display().to_string(), folder, glyph));
        }
    }
    html.push_str(&place(current, "trash:///", "Trash", "trash"));
    html.push_str("</nav>");
    if !app.places.pins.is_empty() || !app.places_healthy {
        html.push_str("<h4>Pinned</h4><nav>");
        for pin in &app.places.pins {
            html.push_str(&format!(
                "<div class=pinrow>{}<button class=quiet data-action=unpin data-arg='{}' title='Remove pin'>×</button></div>",
                place(current, &pin.uri, &pin.name, "folder"),
                esc(&pin.uri)
            ));
        }
        if !app.places_healthy {
            html.push_str(&button("reset-places", "Reset saved places"));
        }
        html.push_str("</nav>");
    }
    if !app.browser.devices.is_empty() {
        html.push_str("<h4>Devices</h4><nav>");
        for device in &app.browser.devices {
            let action = if device.location.is_some() {
                "go"
            } else {
                "mount"
            };
            let arg = device
                .location
                .as_ref()
                .map_or(device.id.as_str(), |location| location.0.as_str());
            html.push_str(&format!(
                "<div class=pinrow><button class=place data-action={action} data-arg='{}'>{}<span>{}</span></button>",
                esc(arg),
                icon("monitor"),
                esc(&device.name)
            ));
            if device.can_unmount {
                html.push_str(&format!(
                    "<button class=quiet data-action=unmount data-arg='{}' title='Safely unmount'>⏏</button>",
                    esc(&device.id)
                ));
            }
            if let Some((total, free)) = device.space {
                html.push_str(&format!(
                    "<small class=disk-space>{} free of {}</small>",
                    size(free),
                    size(total)
                ));
            }
            html.push_str("</div>");
        }
        html.push_str("</nav>");
    }
    html.push_str("<h4>Network</h4><nav>");
    html.push_str(&place(current, "network:///", "Browse network", "globe"));
    html.push_str("<button class=place data-action=connect>");
    html.push_str(&icon("plus"));
    html.push_str("<span>Connect to server…</span></button></nav>");
    html
}

fn place(current: &str, uri: &str, label: &str, glyph: &str) -> String {
    format!(
        "<button class='place{}' data-action=go data-arg='{}'>{}<span>{}</span></button>",
        if current == uri { " active" } else { "" },
        esc(uri),
        icon(glyph),
        esc(label)
    )
}

fn toolbar(app: &App) -> String {
    let pinned = app
        .places
        .pins
        .iter()
        .any(|pin| pin.uri == app.browser.current.0);

    format!(
        "<header><div class=nav-group><button class=icon-btn data-action=back title='Back (Alt+Left)'>{}</button><button class=icon-btn data-action=forward title='Forward (Alt+Right)'>{}</button><button class=icon-btn data-action=up title='Parent folder (Alt+Up)'>↑</button></div><input id=address data-submit=location value='{}' aria-label='Folder path or server address'/><button class=icon-btn data-action=pin title='Pin this folder'>{}</button><button class=icon-btn data-action=refresh title='Refresh (Ctrl+R)'>↻</button><input id=search data-key=search value='{}' placeholder='Search' aria-label='Search this folder'/></header>",
        icon("chevron-left"),
        icon("chevron-right"),
        esc(&app.browser.current.label()),
        if pinned { "★" } else { "☆" },
        esc(&app.browser.query)
    )
}

/// New folder / paste always; item actions only while something is selected.
fn actions(app: &App) -> String {
    let mut html = String::from("<div class=actions>");
    html.push_str(&format!(
        "<button data-action=new-folder>{}<span>New folder</span></button>",
        icon("plus")
    ));
    if app.clipboard.is_some() {
        html.push_str(&button("paste", "Paste"));
    }

    if let Some(entry) = app.browser.selection() {
        html.push_str(&format!(
            "<span class=spacer></span><span class=sel-name>{}</span>{}{}{}{}{}",
            esc(&entry.name),
            button("open", "Open"),
            button("copy", "Copy"),
            button("cut", "Cut"),
            button("rename", "Rename"),
            "<button class=danger data-action=trash>Trash</button>"
        ));
    }

    html.push_str("</div>");
    html
}

pub fn listing(app: &App) -> String {
    let sorted = |sort: Sort, action: &str, label: &str| {
        format!(
            "<button class='{}' data-action=sort data-arg={action}>{label}</button>",
            if app.browser.sort == sort { "on" } else { "" }
        )
    };
    let mut html = actions(app);
    html.push_str(&format!(
        "<div class=columns>{}{}{}</div>",
        sorted(Sort::Name, "name", "Name"),
        sorted(Sort::Modified, "modified", "Modified"),
        sorted(Sort::Size, "size", "Size")
    ));

    for (index, entry) in app
        .browser
        .visible()
        .into_iter()
        .skip(app.browser.page * 100)
        .take(100)
    {
        let selected = if app.browser.selected == Some(index) {
            " selected"
        } else {
            ""
        };

        let glyph = if entry.directory { "folder" } else { "file" };
        html.push_str(&format!(
            "<div class='file-row{selected}' data-action=select data-arg={index}><span class=file-name>{}<span>{}</span></span><span class=meta>{}</span><span class=meta>{}</span></div>",
            icon(glyph),
            esc(&entry.name),
            modified(entry),
            if entry.directory { "—".into() } else { size(entry.size) },
        ));
    }

    if app.browser.visible().is_empty() {
        html.push_str(if app.browser.busy {
            "<div class=empty>Opening folder…</div>"
        } else {
            "<div class=empty>This folder is empty.</div>"
        });
    }

    html
}

pub fn status(app: &App) -> String {
    let count = app.browser.visible().len();
    let pager = if count > 100 {
        format!(
            "<button data-action=page-prev>‹</button><span>Page {} / {}</span><button data-action=page-next>›</button>",
            app.browser.page + 1,
            count.div_ceil(100)
        )
    } else {
        String::new()
    };

    let notice = if app.browser.notice.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=notice><span>{}</span><button class=quiet data-action=dismiss>×</button></div>",
            esc(&app.browser.notice)
        )
    };

    format!(
        "{notice}<div class=statusline><span>{count} {}</span>{pager}<span class=spacer></span>{}<button class=quiet data-action=hidden>{}</button></div>",
        if count == 1 { "item" } else { "items" },
        if app.browser.busy {
            format!("Working… {}", button("cancel-job", "Cancel"))
        } else {
            String::new()
        },
        if app.browser.show_hidden {
            "Hide hidden files"
        } else {
            "Show hidden files"
        }
    )
}

fn size(bytes: u64) -> String {
    for (factor, unit) in [
        (1_099_511_627_776u64, "TiB"),
        (1_073_741_824, "GiB"),
        (1_048_576, "MiB"),
        (1024, "KiB"),
    ] {
        if bytes >= factor {
            return format!("{:.1} {unit}", bytes as f64 / factor as f64);
        }
    }
    format!("{bytes} B")
}

fn modified(entry: &Entry) -> String {
    i64::try_from(entry.modified)
        .ok()
        .and_then(|time| gio::glib::DateTime::from_unix_local(time).ok())
        .and_then(|time| time.format("%d %b %Y").ok())
        .map_or_else(|| "—".into(), Into::into)
}
