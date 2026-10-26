//! Exercises the actual thread/channel/GLib boundary without a desktop session.
use desilium_files::{
    backend::{Command, Event, Worker},
    location::Location,
};
use std::time::Duration;
#[test]
fn filesystem_worker_delivers_only_typed_results_to_the_ui_loop() {
    let directory = tempfile::tempdir().expect("directory");
    std::fs::write(directory.path().join("hello.txt"), b"hello").expect("file");
    let (sender, receiver) = calloop::channel::channel();
    let mut worker = Worker::spawn(sender).expect("worker");
    let location = Location::from_path(directory.path()).expect("location");
    worker
        .send(Command::List {
            location: location.clone(),
            generation: 7,
        })
        .expect("submit");
    type Reply = Option<(
        Location,
        u64,
        Result<Vec<desilium_files::model::Entry>, String>,
    )>;
    let mut event_loop: calloop::EventLoop<Reply> = calloop::EventLoop::try_new().expect("loop");
    event_loop
        .handle()
        .insert_source(receiver, |event, _, result| {
            if let calloop::channel::Event::Msg(Event::Listed {
                location,
                generation,
                result: entries,
            }) = event
            {
                *result = Some((location, generation, entries));
            }
        })
        .expect("source");
    let mut result = None;
    for _ in 0..3 {
        if result.is_some() {
            break;
        }
        event_loop
            .dispatch(Some(Duration::from_secs(2)), &mut result)
            .expect("dispatch");
    }
    let (received, generation, entries) = result.expect("worker response");
    assert_eq!(received, location);
    assert_eq!(generation, 7);
    let entries = entries.expect("listing");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "hello.txt");
}
