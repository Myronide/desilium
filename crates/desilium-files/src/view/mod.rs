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
    desilium_ui::icon_svg(name, 18, "")
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
fn sidebar(app: &App) -> String {
    let mut html = format!(
        "<div class=brand><span class=brandmark>D</span><div><strong>Doria</strong><small>DESILIUM FILES</small></div></div><h4>PLACES</h4>{}",
        place(&app.home.display().to_string(), "Home", "folder")
    );
    for (folder, name) in [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Pictures", "Pictures"),
        ("Videos", "Videos"),
        ("Music", "Music"),
    ] {
        let path = app.home.join(folder);
        if path.is_dir() {
            html.push_str(&place(&path.display().to_string(), name, "folder"));
        }
    }
    html.push_str(&place("file:///", "File system", "monitor"));
    html.push_str(&place("trash:///", "Trash", "trash"));
    html.push_str("<h4>PINNED FOLDERS</h4>");
    for pin in &app.places.pins {
        html.push_str(&format!("<div class=pinrow>{}<button class=quiet data-action=unpin data-arg='{}' title='Remove pin'>×</button></div>",place(&pin.uri,&pin.name,"folder"),esc(&pin.uri)));
    }
    if app.places.pins.is_empty() {
        html.push_str("<p class=hint>Pin any open folder with the star above.</p>");
    }
    if !app.places_healthy {
        html.push_str(&button("reset-places", "Reset saved places"));
    }
    html.push_str("<h4>DEVICES</h4>");
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
        html.push_str(&format!("<div class=pinrow><button class=place data-action={action} data-arg='{}'>{}<span>{}</span></button>",esc(arg),icon("monitor"),esc(&device.name)));
        if let Some((total, free)) = device.space {
            html.push_str(&format!(
                "<small class=disk-space>{} free / {}</small>",
                size(free),
                size(total)
            ));
        }
        if device.can_unmount {
            html.push_str(&format!("<button class=quiet data-action=unmount data-arg='{}' title='Safely unmount'>⏏</button>",esc(&device.id)));
        }
        html.push_str("</div>");
    }
    if app.browser.devices.is_empty() {
        html.push_str("<p class=hint>No removable drives detected.</p>");
    }
    html.push_str("<h4>NETWORK</h4>");
    html.push_str(&place("network:///", "Browse network", "wifi"));
    html.push_str(&button("connect", "Connect to server…"));
    html
}
fn place(uri: &str, label: &str, glyph: &str) -> String {
    format!(
        "<button class=place data-action=go data-arg='{}'>{}<span>{}</span></button>",
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
    let navigation = format!(
        "<div class=navigation>{}{}{}<input id=address data-submit=location value='{}' aria-label='Folder path or server address'/><button data-action=pin title='Pin this folder'>{}</button>{}</div>",
        button("back", "←"),
        button("forward", "→"),
        button("up", "↑"),
        esc(&app.browser.current.label()),
        if pinned { "★" } else { "☆" },
        button("refresh", "↻")
    );
    format!(
        "<header>{navigation}<div class=tools>{}{}{}{}{}{}<span class=spacer></span><button data-action=hidden>{}</button><label class=search-label>Filter</label><input id=search data-key=search value='{}' placeholder='Search this folder' aria-label='Search this folder'/></div></header>",
        button("new-folder", "New folder"),
        button("copy", "Copy"),
        button("cut", "Cut"),
        button("paste", "Paste"),
        button("rename", "Rename"),
        button("trash", "Trash"),
        if app.browser.show_hidden {
            "Hide hidden"
        } else {
            "Show hidden"
        },
        esc(&app.browser.query)
    )
}
pub fn listing(app: &App) -> String {
    let mut html = String::from(
        "<div class=columns><button data-action=sort data-arg=name>Name</button><button data-action=sort data-arg=size>Size</button><button data-action=sort data-arg=modified>Modified</button><span>Type</span></div>",
    );
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
        html.push_str(&format!("<div class='file-row{selected}' data-action=select data-arg={index}><span class=file-name>{}<span>{}</span></span><span>{}</span><span>{}</span><span class=kind>{}</span><button class=open data-action=open data-arg={index}>Open</button></div>",icon(glyph),esc(&entry.name),if entry.directory{"—".into()}else{size(entry.size)},modified(entry),esc(&entry.kind)));
    }
    if app.browser.visible().is_empty() {
        html.push_str(if app.browser.busy {
            "<div class=empty>Opening folder…</div>"
        } else {
            "<div class=empty>No items to show.</div>"
        });
    }
    html
}
pub fn status(app: &App) -> String {
    let count = app.browser.visible().len();
    let sort = match app.browser.sort {
        Sort::Name => "Name",
        Sort::Size => "Size",
        Sort::Modified => "Modified",
    };
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
            "<div class=notice>{}<button data-action=dismiss>×</button></div>",
            esc(&app.browser.notice)
        )
    };
    format!(
        "{notice}<div class=statusline><span>{count} items · Sorted by {sort}</span>{pager}<span class=spacer></span>{}</div>",
        if app.browser.busy {
            format!("Working… {}", button("cancel-job", "Cancel"))
        } else {
            "Doria · Your files, your places".into()
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
