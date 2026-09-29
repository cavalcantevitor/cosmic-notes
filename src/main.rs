use cosmic_notes::app::AppModel;
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault_path = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("CosmicNotes")
    });

    println!("Launching COSMIC Notes with vault at: {:?}", vault_path);

    let settings = cosmic::app::Settings::default();
    cosmic::app::run::<AppModel>(settings, vault_path)?;

    Ok(())
}
