//! The background threads of the store: searches on one, downloads and installs on another, so a
//! slow download never delays a search and neither ever blocks the window.
//!
//! Like the thumbnail worker, a job goes in over a channel and its result is handed to a callback
//! that wakes the event loop.

use std::{
    ffi::OsString,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use super::{
    Entry, Kind, Listing, Sort, StoreDirs, StoreError,
    install::{Scratch, install_file, sweep},
    ocs::{self, is_https, parse_listing, search_url},
    process::{Limits, run},
};

const USER_AGENT: &str = concat!("Desilium-Settings/", env!("CARGO_PKG_VERSION"));
/// Largest download.
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
/// Largest answer of the API.
const MAX_ANSWER: u64 = 4 * 1024 * 1024;

/// Work for the background threads.
#[derive(Debug)]
pub enum Job {
    /// One page of a search.
    Search {
        /// Matches the answer to the request that is still wanted.
        seq: u64,
        /// What to list.
        kind: Kind,
        /// Search text (may be empty).
        query: String,
        /// Page, counted from 0.
        page: u32,
    },
    /// Download and install an entry.
    Install {
        /// What it is.
        kind: Kind,
        /// The entry.
        entry: Entry,
    },
}

/// What a job produced.
#[derive(Debug)]
pub enum Done {
    /// A search finished.
    Listing {
        /// The request it answers.
        seq: u64,
        /// The page or why there is none.
        outcome: Result<Listing, StoreError>,
    },
    /// An install finished.
    Installed {
        /// What kind of theme it was.
        kind: Kind,
        /// The entry's store id.
        id: String,
        /// The names of the installed themes or why nothing was installed.
        outcome: Result<Vec<String>, StoreError>,
    },
}

/// Handle to the background threads.
pub struct StoreWorker {
    searches: Sender<Job>,
    installs: Sender<Job>,
}

impl StoreWorker {
    /// Starts the threads; every result goes to `deliver` (called on a worker thread).
    pub fn spawn(dirs: StoreDirs, deliver: impl Fn(Done) + Send + Clone + 'static) -> Self {
        let start = |name: &str, dirs: StoreDirs, deliver: Box<dyn Fn(Done) + Send>| {
            let (tx, rx) = mpsc::channel::<Job>();
            let spawned = thread::Builder::new().name(name.to_owned()).spawn(move || {
                sweep(&dirs.work);
                while let Ok(job) = rx.recv() {
                    deliver(run_job(job, &dirs));
                }
            });
            if let Err(error) = spawned {
                tracing::warn!(%error, "the theme store thread did not start");
            }
            tx
        };
        Self {
            searches: start("store-search", dirs.clone(), Box::new(deliver.clone())),
            installs: start("store-install", dirs, Box::new(deliver)),
        }
    }

    /// A worker whose jobs land in the returned receiver, for tests.
    pub fn detached() -> (Self, Receiver<Job>, Receiver<Job>) {
        let (searches, search_rx) = mpsc::channel();
        let (installs, install_rx) = mpsc::channel();
        (Self { searches, installs }, search_rx, install_rx)
    }

    /// Queues `job`; false when the thread is gone.
    pub fn submit(&self, job: Job) -> bool {
        let sender = match job {
            Job::Search { .. } => &self.searches,
            Job::Install { .. } => &self.installs,
        };
        sender.send(job).is_ok()
    }
}

fn run_job(job: Job, dirs: &StoreDirs) -> Done {
    match job {
        Job::Search {
            seq,
            kind,
            query,
            page,
        } => Done::Listing {
            seq,
            outcome: fetch_text(&search_url(kind, &query, Sort::default(), page))
                .and_then(|text| parse_listing(&text)),
        },
        Job::Install { kind, entry } => Done::Installed {
            kind,
            id: entry.id.clone(),
            outcome: install_entry(kind, &entry, dirs),
        },
    }
}

/// The curl options every request shares: https only (also after redirects), bounded time.
fn curl_args(max_time: &str, max_size: &str) -> Vec<OsString> {
    [
        "-sS",
        "-f",
        "-L",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--connect-timeout",
        "15",
        "--max-time",
        max_time,
        "--max-filesize",
        max_size,
        "--user-agent",
        USER_AGENT,
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn network_error(stderr: &str) -> StoreError {
    let first = stderr.lines().next().unwrap_or("unknown error");
    StoreError::Network(first.chars().take(160).collect())
}

/// GETs `url` and returns the text of the answer.
pub fn fetch_text(url: &str) -> Result<String, StoreError> {
    if !is_https(url) {
        return Err(StoreError::BadResponse("not an https link".to_owned()));
    }
    let mut args = curl_args("30", &MAX_ANSWER.to_string());
    args.extend(["-H", "Accept: application/json", "--url", url].map(OsString::from));
    let refs: Vec<&std::ffi::OsStr> = args.iter().map(OsString::as_os_str).collect();
    let done = run(
        "curl",
        &refs,
        &Limits {
            timeout: Duration::from_secs(40),
            max_stdout: MAX_ANSWER,
            watch: None,
        },
    )?;
    if !done.success {
        return Err(network_error(&done.stderr));
    }
    String::from_utf8(done.stdout).map_err(|_| StoreError::BadResponse("not text".to_owned()))
}

/// Downloads `url` to `destination`, at most [`MAX_DOWNLOAD`] bytes.
fn download(url: &str, destination: &std::path::Path) -> Result<(), StoreError> {
    if !is_https(url) {
        return Err(StoreError::NoDownload);
    }
    let mut args = curl_args("900", &MAX_DOWNLOAD.to_string());
    args.push(OsString::from("-o"));
    args.push(destination.as_os_str().to_owned());
    args.push(OsString::from("--url"));
    args.push(OsString::from(url));
    let refs: Vec<&std::ffi::OsStr> = args.iter().map(OsString::as_os_str).collect();
    let done = run(
        "curl",
        &refs,
        &Limits {
            timeout: Duration::from_secs(960),
            max_stdout: 4096,
            watch: Some((destination.to_path_buf(), MAX_DOWNLOAD)),
        },
    )?;
    if done.success {
        Ok(())
    } else {
        Err(network_error(&done.stderr))
    }
}

fn install_entry(kind: Kind, entry: &Entry, dirs: &StoreDirs) -> Result<Vec<String>, StoreError> {
    if !dirs.can_install() {
        return Err(StoreError::NoHome);
    }
    let mut file = entry.best_file(kind).cloned();
    if file.is_none() {
        // Some answers list entries without their files; the detail request carries them.
        let url = ocs::detail_url(&entry.id).ok_or(StoreError::NoDownload)?;
        let detail = parse_listing(&fetch_text(&url)?)?;
        file = detail
            .entries
            .into_iter()
            .find(|candidate| candidate.id == entry.id)
            .and_then(|candidate| candidate.best_file(kind).cloned());
    }
    let file = file.ok_or(StoreError::NoDownload)?;
    let scratch = Scratch::new(&dirs.work)?;
    let path = scratch.path().join("download");
    download(&file.url, &path)?;
    install_file(
        &path,
        &scratch,
        kind,
        &entry.id,
        (&file.name, &entry.name),
        dirs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_go_to_the_matching_thread() {
        let (worker, searches, installs) = StoreWorker::detached();
        assert!(worker.submit(Job::Search {
            seq: 1,
            kind: Kind::Icons,
            query: String::new(),
            page: 0,
        }));
        assert!(searches.try_recv().is_ok());
        assert!(installs.try_recv().is_err());
        drop(searches);
        assert!(
            !worker.submit(Job::Search {
                seq: 2,
                kind: Kind::Icons,
                query: String::new(),
                page: 0,
            }),
            "a gone thread is reported"
        );
    }

    #[test]
    fn only_https_links_are_fetched_or_downloaded() {
        assert!(matches!(
            fetch_text("http://example.org/a"),
            Err(StoreError::BadResponse(_))
        ));
        let dir = tempfile::tempdir().expect("dir");
        assert!(matches!(
            download("file:///etc/passwd", &dir.path().join("x")),
            Err(StoreError::NoDownload)
        ));
    }

    #[test]
    fn requests_never_leave_https() {
        let args = curl_args("30", "100");
        let text: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = text.iter().position(|a| a == "--proto").expect("proto");
        assert_eq!(text[at + 1], "=https");
        let at = text
            .iter()
            .position(|a| a == "--proto-redir")
            .expect("redir");
        assert_eq!(text[at + 1], "=https");
    }
}
