use wc_core::{Placement, Request};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let request = match args.next().as_deref() {
        Some("status") | None => Request::Get,
        Some("left") => Request::Place(Placement::Left),
        Some("right") => Request::Place(Placement::Right),
        Some("fullscreen") => Request::Place(Placement::Fullscreen),
        Some("maximize") => Request::Place(Placement::Maximize),
        Some("restore") => Request::Place(Placement::Restore),
        Some("cycle") => Request::Cycle,
        Some("close") => Request::Close,
        Some("apply") => {
            let path = args.next().ok_or("wcctl apply DATEI.toml")?;
            if !std::path::Path::new(&path).is_file() {
                return Err("Konfigurationsdatei existiert nicht".into());
            }
            Request::Apply(Box::new(wc_core::load_from(std::path::Path::new(&path))?))
        }
        Some("--help" | "-h") => {
            println!(
                "wcctl [status|left|right|maximize|fullscreen|restore|cycle|close|apply DATEI.toml]\nWC_SOCKET wählt die Instanz."
            );
            return Ok(());
        }
        Some(other) => return Err(format!("Unbekannter Befehl: {other}").into()),
    };
    let snapshot = wc_core::request(&request)?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}
