use crate::Smallvil;
use smithay::reexports::calloop::{EventLoop, channel};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    sync::mpsc,
    time::Duration,
};
use wc_core::{MAX_MESSAGE, Request, Response};

/// The listener thread only decodes messages. All compositor changes run on calloop.
pub fn install(
    event_loop: &mut EventLoop<Smallvil>,
) -> Result<SocketGuard, Box<dyn std::error::Error>> {
    let path = wc_core::socket_path()?;
    let listener = UnixListener::bind(&path).map_err(|e| {
        format!(
            "{}: {e}. Läuft WC bereits? Für mehrere Instanzen WC_SOCKET setzen.",
            path.display()
        )
    })?;
    let guard = SocketGuard(path);
    std::fs::set_permissions(&guard.0, std::fs::Permissions::from_mode(0o600))?;
    let (tx, rx) = channel::channel::<(Request, mpsc::SyncSender<Response>)>();
    event_loop.handle().insert_source(rx, |event, _, state| {
        if let channel::Event::Msg((request, reply)) = event {
            let result = match request {
                Request::Get => Ok(()),
                Request::Apply(settings) => state.apply_settings(*settings),
                Request::Place(placement) => state.place_focused(placement),
                Request::Cycle => {
                    state.cycle();
                    Ok(())
                }
                Request::Close => {
                    state.close_focused();
                    Ok(())
                }
            };
            let response = match result {
                Ok(()) => Response {
                    snapshot: Some(state.snapshot()),
                    error: None,
                },
                Err(error) => Response {
                    snapshot: None,
                    error: Some(error),
                },
            };
            let _ = reply.send(response);
        }
    })?;
    std::thread::Builder::new()
        .name("wc-control".into())
        .spawn(move || {
            for connection in listener.incoming() {
                let Ok(mut stream) = connection else { break };
                let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
                let mut raw = String::new();
                let result = BufReader::new((&stream).take(MAX_MESSAGE + 1)).read_line(&mut raw);
                let response =
                    if result.is_err() || raw.len() as u64 > MAX_MESSAGE || !raw.ends_with('\n') {
                        Response {
                            snapshot: None,
                            error: Some("Ungültige oder zu große Anfrage".into()),
                        }
                    } else {
                        match serde_json::from_str::<Request>(&raw) {
                            Ok(request) => {
                                let (reply, receive) = mpsc::sync_channel(1);
                                if tx.send((request, reply)).is_err() {
                                    break;
                                }
                                receive
                                    .recv_timeout(Duration::from_secs(1))
                                    .unwrap_or(Response {
                                        snapshot: None,
                                        error: Some("Compositor antwortet nicht".into()),
                                    })
                            }
                            Err(e) => Response {
                                snapshot: None,
                                error: Some(e.to_string()),
                            },
                        }
                    };
                let _ = serde_json::to_writer(&mut stream, &response);
                let _ = stream.write_all(b"\n");
            }
        })?;
    Ok(guard)
}
pub struct SocketGuard(std::path::PathBuf);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
