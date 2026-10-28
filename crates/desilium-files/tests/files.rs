// Test code: a failing unwrap or expect is the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use desilium_files::{
    backend::files,
    location::{Location, valid_name},
    model::{Browser, Sort},
    places::Places,
};
use gio::prelude::CancellableExt;
use std::{fs, path::Path};
fn location(path: &Path) -> Location {
    Location::from_path(path).expect("absolute path")
}
#[test]
fn local_file_operations_are_no_overwrite_and_keep_symlinks() {
    let temp = tempfile::tempdir().expect("dir");
    let root = location(temp.path());
    let cancel = gio::Cancellable::new();
    files::create_folder(&root, "source", &cancel).expect("create");
    fs::write(temp.path().join("source/note.txt"), b"hello").expect("file");
    std::os::unix::fs::symlink("note.txt", temp.path().join("source/link")).expect("link");
    files::create_folder(&root, "target", &cancel).expect("target");
    let source = location(&temp.path().join("source"));
    let target = location(&temp.path().join("target"));
    files::transfer(&source, &target, false, &cancel).expect("copy tree");
    assert_eq!(
        fs::read(temp.path().join("target/source/note.txt")).expect("read"),
        b"hello"
    );
    assert!(
        fs::symlink_metadata(temp.path().join("target/source/link"))
            .expect("metadata")
            .is_symlink()
    );
    assert!(files::transfer(&source, &target, false, &cancel).is_err());
    assert!(files::transfer(&source, &source, false, &cancel).is_err());
    files::rename(
        &location(&temp.path().join("target/source/note.txt")),
        "renamed.txt",
        &cancel,
    )
    .expect("rename");
    assert!(temp.path().join("target/source/renamed.txt").exists());
    assert!(files::create_folder(&root, "../outside", &cancel).is_err());
    cancel.cancel();
    assert!(files::list(&root, &cancel).is_err());
}
#[test]
fn browser_filters_hidden_names_and_preserves_history() {
    let temp = tempfile::tempdir().expect("dir");
    fs::create_dir(temp.path().join("Zoo")).expect("folder");
    fs::write(temp.path().join("alpha.txt"), b"hello").expect("file");
    fs::write(temp.path().join(".secret"), b"secret").expect("hidden");
    let root = location(temp.path());
    let mut browser = Browser::new(root.clone());
    browser.entries = files::list(&root, &gio::Cancellable::new()).expect("list");
    assert_eq!(browser.visible().len(), 2);
    assert_eq!(browser.visible()[0].1.name, "Zoo");
    browser.show_hidden = true;
    assert_eq!(browser.visible().len(), 3);
    browser.query = "ALPHA".into();
    assert_eq!(browser.visible().len(), 1);
    browser.sort = Sort::Size;
    assert_eq!(browser.visible()[0].1.size, 5);
    browser.navigate(location(&temp.path().join("Zoo")));
    assert!(browser.back());
    assert_eq!(browser.current, root);
    assert!(browser.forward());
    assert_eq!(browser.current.path(), Some(temp.path().join("Zoo")));
}
#[test]
fn address_validation_never_accepts_saved_credentials_or_commands() {
    for address in [
        "smb://server/share",
        "sftp://alice@host/folder",
        "davs://server/share",
        "network:///",
        "trash:///",
    ] {
        assert!(
            Location::parse(address, Path::new("/home/test")).is_ok(),
            "{address}"
        );
    }
    for address in [
        "sftp://alice:secret@host/folder",
        "smb://user:pass@host/share",
        "https://example.com",
        "",
        "sftp:///",
    ] {
        assert!(
            Location::parse(address, Path::new("/home/test")).is_err(),
            "{address}"
        );
    }
    assert_eq!(
        Location::parse("~/Downloads", Path::new("/home/test"))
            .expect("tilde")
            .path(),
        Some("/home/test/Downloads".into())
    );
    for name in ["", ".", "..", "a/b", "bad\0name"] {
        assert!(valid_name(name).is_err());
    }
    assert!(valid_name("a file with spaces").is_ok());
}
#[test]
fn pins_are_atomic_and_a_corrupt_file_is_not_overwritten_on_load() {
    let temp = tempfile::tempdir().expect("dir");
    let path = temp.path().join("places.toml");
    let mut places = Places::load(&path).expect("defaults");
    let root = location(temp.path());
    places.toggle(&root, "Work".into());
    places.save(&path).expect("save");
    assert_eq!(Places::load(&path).expect("load").pins, places.pins);
    places.toggle(&root, "Work".into());
    assert!(places.pins.is_empty());
    fs::write(&path, "pins = [").expect("corrupt");
    assert!(Places::load(&path).is_err());
    assert_eq!(fs::read_to_string(&path).expect("read"), "pins = [");
}
#[test]
fn markup_escapes_file_names_and_server_errors() {
    assert_eq!(
        desilium_files::view::esc("<script>'\"&"),
        "&lt;script&gt;&#39;&quot;&amp;"
    );
}
