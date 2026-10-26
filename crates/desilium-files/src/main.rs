use std::process::ExitCode;
fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    match arguments() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Doria: {error}");
            ExitCode::FAILURE
        }
    }
}
fn arguments() -> Result<(), desilium_files::runtime::Error> {
    use desilium_files::runtime::{self, Error};
    let mut args = std::env::args().skip(1);
    let first = args.next();
    match first.as_deref() {
        Some("--help" | "-h") => {
            println!(
                "Doria — Desilium Files\n\ndoria [FOLDER_OR_URI]\ndoria --render OUTPUT.png FOLDER\n\nSMB, SFTP and WebDAV use the installed GVfs backends. Passwords are entered in a native dialog and are never saved."
            );
            Ok(())
        }
        Some("--render") => {
            let output = args
                .next()
                .ok_or_else(|| Error::Message("--render needs OUTPUT.png and FOLDER".into()))?;
            let folder = args
                .next()
                .ok_or_else(|| Error::Message("--render needs a folder".into()))?;
            runtime::render(folder.into(), output.into())
        }
        Some(flag) if flag.starts_with('-') => {
            Err(Error::Message(format!("Unknown option {flag}")))
        }
        _ => runtime::run(first),
    }
}
