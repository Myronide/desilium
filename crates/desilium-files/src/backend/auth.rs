use super::{Event, EventSender};
use gio::prelude::*;
use std::{sync::mpsc, time::Duration};

/// Kept in memory for this operation only; never Debug, logged or persisted.
#[derive(Default)]
pub struct Credentials {
    pub username: String,
    pub domain: String,
    pub password: String,
}
pub struct Challenge {
    pub username: String,
    pub domain: String,
    pub reply: mpsc::SyncSender<Option<Credentials>>,
}
pub fn operation(events: EventSender) -> gio::MountOperation {
    let operation = gio::MountOperation::new();
    operation.set_password_save(gio::PasswordSave::Never);
    let question_events = events.clone();
    operation.connect_local("ask-question", false, move |values| {
        let operation = values
            .first()
            .and_then(|value| value.get::<gio::MountOperation>().ok())?;
        let message = values.get(1).and_then(|value| value.get::<String>().ok())?;
        let choices = values
            .get(2)
            .and_then(|value| value.get::<gio::glib::StrV>().ok())?
            .iter()
            .map(|choice| choice.to_string())
            .collect();
        let (reply, response) = mpsc::sync_channel(1);
        if question_events
            .send(Event::Question {
                message,
                choices,
                reply,
            })
            .is_err()
        {
            operation.reply(gio::MountOperationResult::Aborted);
            return None;
        }
        match response.recv_timeout(Duration::from_secs(120)) {
            Ok(Some(choice)) => {
                operation.set_choice(choice);
                operation.reply(gio::MountOperationResult::Handled);
            }
            _ => operation.reply(gio::MountOperationResult::Aborted),
        }
        None
    });
    operation.connect_ask_password(move |operation, _message, username, domain, _flags| {
        let (reply, response) = mpsc::sync_channel(1);
        if events
            .send(Event::Authenticate(Challenge {
                username: username.into(),
                domain: domain.into(),
                reply,
            }))
            .is_err()
        {
            operation.reply(gio::MountOperationResult::Aborted);
            return;
        }
        let Ok(Some(credentials)) = response.recv_timeout(Duration::from_secs(120)) else {
            operation.reply(gio::MountOperationResult::Aborted);
            return;
        };
        operation.set_username(Some(&credentials.username));
        operation.set_domain(Some(&credentials.domain));
        operation.set_password(Some(&credentials.password));
        operation.reply(gio::MountOperationResult::Handled);
    });
    operation
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sign_in_and_host_confirmation_round_trip_without_persisting_passwords() {
        let (sender, receiver) = calloop::channel::channel();
        let thread = std::thread::spawn(move || {
            let context = gio::glib::MainContext::new();
            context
                .with_thread_default(|| {
                    let operation = operation(sender);
                    let choices = gio::glib::StrV::from(["Cancel", "Trust this host"]);
                    operation.emit_by_name::<()>("ask-question", &[&"Test fingerprint", &choices]);
                    assert_eq!(operation.choice(), 1);
                    operation.emit_by_name::<()>(
                        "ask-password",
                        &[
                            &"Test login",
                            &"guest",
                            &"",
                            &gio::AskPasswordFlags::NEED_PASSWORD,
                        ],
                    );
                    assert_eq!(operation.username().as_deref(), Some("alice"));
                    assert_eq!(operation.password().as_deref(), Some("test-only"));
                    assert_eq!(operation.password_save(), gio::PasswordSave::Never);
                })
                .expect("thread context");
        });
        let mut event_loop: calloop::EventLoop<usize> =
            calloop::EventLoop::try_new().expect("loop");
        event_loop
            .handle()
            .insert_source(receiver, |event, _, count| match event {
                calloop::channel::Event::Msg(Event::Question { choices, reply, .. }) => {
                    assert_eq!(choices.len(), 2);
                    reply.send(Some(1)).expect("host reply");
                    *count += 1;
                }
                calloop::channel::Event::Msg(Event::Authenticate(challenge)) => {
                    challenge
                        .reply
                        .send(Some(Credentials {
                            username: "alice".into(),
                            password: "test-only".into(),
                            domain: String::new(),
                        }))
                        .expect("credentials reply");
                    *count += 1;
                }
                _ => {}
            })
            .expect("source");
        let mut count = 0;
        for _ in 0..4 {
            if count == 2 {
                break;
            }
            event_loop
                .dispatch(Some(Duration::from_secs(2)), &mut count)
                .expect("dispatch");
        }
        assert_eq!(count, 2);
        thread.join().expect("authentication worker");
    }
}
