//! Daemon entry point: connects to the session bus and serves both notification interfaces.

use std::process::ExitCode;

use desilium_notifications::{ServiceError, dbus::Service};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    match async_io::block_on(run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "notification service stopped");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), ServiceError> {
    let service = Service::connect(None).await?;
    tracing::info!("notification service ready on the session bus");
    service.run().await;
    Ok(())
}
