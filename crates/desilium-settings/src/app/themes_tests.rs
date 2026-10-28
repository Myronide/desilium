//! Behaviour of the "Get new themes" page against a detached store worker: what is asked of the
//! network and what the answers do.

use std::{sync::mpsc::Receiver, time::Instant};

use desilium_ui::{Rect, UiAction, UiEvent};

use super::{App, tests::app_in};
use crate::store::{Done, Entry, Job, Kind, Listing, Load, StoreError, StoreWorker};

fn action(name: &str, arg: &str) -> UiEvent {
    UiEvent::Action(UiAction {
        action: name.to_owned(),
        arg: Some(arg.to_owned()),
        element_id: None,
        rect: Rect::default(),
    })
}

fn entry(id: &str) -> Entry {
    Entry {
        id: id.to_owned(),
        name: format!("Theme {id}"),
        author: String::new(),
        downloads: 1,
        score: 1,
        detail_page: None,
        files: Vec::new(),
    }
}

fn with_worker() -> (tempfile::TempDir, App, Receiver<Job>, Receiver<Job>) {
    let (dir, mut app) = app_in();
    let (worker, searches, installs) = StoreWorker::detached();
    app.set_store_worker(worker);
    (dir, app, searches, installs)
}

fn ready(app: &mut App, entries: Vec<Entry>, total: u64) {
    let seq = app.model.store.seq;
    app.on_store(
        Done::Listing {
            seq,
            outcome: Ok(Listing { total, entries }),
        },
        Instant::now(),
    );
}

#[test]
fn opening_the_page_searches_once_and_answers_to_old_searches_are_dropped() {
    let (_dir, mut app, searches, _installs) = with_worker();
    let now = Instant::now();
    app.handle_event(&action("nav", "themes"), now);
    assert!(matches!(
        searches.try_recv(),
        Ok(Job::Search {
            seq: 1,
            kind: Kind::Icons,
            page: 0,
            ..
        })
    ));
    assert_eq!(app.model.store.load, Load::Loading);
    app.handle_event(&action("nav", "appearance"), now);
    app.handle_event(&action("nav", "themes"), now);
    assert!(
        searches.try_recv().is_err(),
        "leaving and coming back does not search again"
    );
    // A newer search makes the answer to the first one stale.
    app.handle_event(&action("store-reload", ""), now);
    let stale = Done::Listing {
        seq: 1,
        outcome: Ok(Listing {
            total: 1,
            entries: vec![entry("1")],
        }),
    };
    assert!(app.on_store(stale, now).is_empty());
    assert_eq!(app.model.store.load, Load::Loading);
    ready(&mut app, vec![entry("2")], 1);
    assert_eq!(app.model.store.load, Load::Ready);
    assert_eq!(app.model.store.listing.entries[0].id, "2");
}

#[test]
fn tabs_search_and_paging_ask_for_the_right_page() {
    let (_dir, mut app, searches, _installs) = with_worker();
    let now = Instant::now();
    app.handle_event(&action("nav", "themes"), now);
    let _ = searches.try_recv();
    ready(
        &mut app,
        (0..10).map(|n| entry(&n.to_string())).collect(),
        25,
    );
    app.handle_event(&action("store-page", "next"), now);
    assert!(matches!(
        searches.try_recv(),
        Ok(Job::Search { page: 1, .. })
    ));
    app.handle_event(&action("store-page", "prev"), now);
    app.handle_event(&action("store-page", "prev"), now);
    assert!(matches!(
        searches.try_recv(),
        Ok(Job::Search { page: 0, .. })
    ));
    assert!(searches.try_recv().is_err(), "no page before the first");
    app.handle_event(&action("store-tab", "cursors"), now);
    assert!(matches!(
        searches.try_recv(),
        Ok(Job::Search {
            kind: Kind::Cursors,
            page: 0,
            ..
        })
    ));
    app.handle_event(
        &UiEvent::TextChanged {
            element_id: None,
            key: Some("store-search".into()),
            value: "  blue ".into(),
        },
        now,
    );
    app.handle_event(
        &UiEvent::Submit {
            action: "store-search".into(),
            value: "  blue ".into(),
        },
        now,
    );
    match searches.try_recv() {
        Ok(Job::Search { query, kind, .. }) => {
            assert_eq!((query.as_str(), kind), ("blue", Kind::Cursors));
        }
        other => panic!("{other:?}"),
    }
    app.handle_event(&action("store-tab", "nonsense"), now);
    assert!(searches.try_recv().is_err());
}

#[test]
fn installing_queues_one_job_and_the_result_shows_up_installed() {
    let (dir, mut app, searches, installs) = with_worker();
    let now = Instant::now();
    app.handle_event(&action("nav", "themes"), now);
    let _ = searches.try_recv();
    ready(&mut app, vec![entry("7"), entry("8")], 2);
    app.handle_event(&action("store-install", "5"), now);
    assert!(installs.try_recv().is_err(), "no such row");
    app.handle_event(&action("store-install", "0"), now);
    assert!(
        matches!(installs.try_recv(), Ok(Job::Install { kind: Kind::Icons, entry }) if entry.id == "7")
    );
    assert_eq!(app.model.store.installing.as_deref(), Some("7"));
    app.handle_event(&action("store-install", "1"), now);
    assert!(installs.try_recv().is_err(), "one download at a time");
    // The worker reports success after writing the theme.
    let theme = dir.path().join("data/icons/Fresh");
    std::fs::create_dir_all(&theme).expect("dir");
    std::fs::write(theme.join("index.theme"), "[Icon Theme]\nDirectories=a\n").expect("file");
    std::fs::write(theme.join(crate::store::install::MARKER), "7").expect("marker");
    let refresh = app.on_store(
        Done::Installed {
            kind: Kind::Icons,
            id: "7".into(),
            outcome: Ok(vec!["Fresh".into()]),
        },
        now,
    );
    assert!(!refresh.is_empty());
    assert_eq!(app.model.store.installing, None);
    assert_eq!(app.model.store.installed, ["Fresh"]);
    assert_eq!(app.model.store.installed_from("7"), ["Fresh"]);
    assert!(
        app.model
            .toast
            .as_ref()
            .is_some_and(|toast| toast.message.contains("Fresh"))
    );
}

#[test]
fn a_failed_install_is_a_notice_and_frees_the_page() {
    let (_dir, mut app, searches, _installs) = with_worker();
    let now = Instant::now();
    app.handle_event(&action("nav", "themes"), now);
    let _ = searches.try_recv();
    ready(&mut app, vec![entry("7")], 1);
    app.handle_event(&action("store-install", "0"), now);
    app.on_store(
        Done::Installed {
            kind: Kind::Icons,
            id: "7".into(),
            outcome: Err(StoreError::Unsafe("../x".into())),
        },
        now,
    );
    assert_eq!(app.model.store.installing, None);
    let notice = app.model.notice.as_ref().expect("a notice");
    assert!(
        notice.text.contains("unsafe") && notice.text.contains("../x"),
        "{}",
        notice.text
    );
}

#[test]
fn using_a_theme_sets_the_matching_setting_and_only_installed_names_are_accepted() {
    let (dir, mut app, searches, _installs) = with_worker();
    let now = Instant::now();
    let icons = dir.path().join("data/icons");
    for (name, cursors) in [("Papirus", false), ("Bibata", true)] {
        let theme = icons.join(name);
        std::fs::create_dir_all(theme.join(if cursors { "cursors" } else { "apps" })).expect("dir");
        if !cursors {
            std::fs::write(
                theme.join("index.theme"),
                "[Icon Theme]\nDirectories=apps\n",
            )
            .expect("file");
        }
    }
    let schemes = dir.path().join("data/color-schemes");
    std::fs::create_dir_all(&schemes).expect("dir");
    std::fs::write(
        schemes.join("Night.colors"),
        "[Colors:Window]\nBackgroundNormal=1,2,3\n",
    )
    .expect("file");
    app.handle_event(&action("nav", "themes"), now);
    let _ = searches.try_recv();
    assert_eq!(app.model.store.installed, ["Papirus"]);
    app.handle_event(&action("store-use", "Papirus"), now);
    assert_eq!(
        app.store.get().appearance.icon_theme.as_deref(),
        Some("Papirus")
    );
    app.handle_event(&action("store-use", "../../etc"), now);
    app.handle_event(&action("store-use", "Bibata"), now);
    assert_eq!(
        app.store.get().appearance.icon_theme.as_deref(),
        Some("Papirus"),
        "a cursor name is no icon theme"
    );
    assert_eq!(app.store.get().appearance.cursor_theme, None);

    app.handle_event(&action("store-tab", "cursors"), now);
    app.handle_event(&action("store-use", "Bibata"), now);
    assert_eq!(
        app.store.get().appearance.cursor_theme.as_deref(),
        Some("Bibata")
    );

    app.handle_event(&action("store-tab", "colours"), now);
    app.handle_event(&action("store-use", "Night"), now);
    let appearance = &app.store.get().appearance;
    assert_eq!(appearance.theme, desilium_config_types::KDE_THEME);
    assert_eq!(appearance.kde_scheme.as_deref(), Some("Night"));
    // Both fields changed in one transaction: one Undo brings both back.
    app.handle_event(&action("undo", ""), now);
    let appearance = &app.store.get().appearance;
    assert_eq!(
        (appearance.theme.as_str(), appearance.kde_scheme.as_deref()),
        ("desilium-dark", None)
    );
}

#[test]
fn without_a_helper_the_page_stays_idle_and_installing_is_refused() {
    let (_dir, mut app) = app_in();
    let now = Instant::now();
    app.handle_event(&action("nav", "themes"), now);
    assert_eq!(app.model.store.load, Load::Idle);
    app.model.store.listing = Listing {
        total: 1,
        entries: vec![entry("1")],
    };
    app.model.store.load = Load::Ready;
    app.handle_event(&action("store-install", "0"), now);
    assert_eq!(app.model.store.installing, None);
    assert!(app.model.notice.is_some());
}
