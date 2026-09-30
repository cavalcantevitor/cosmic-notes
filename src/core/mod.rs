pub mod error;
pub mod folder;
pub mod note;
pub mod vault;
pub mod watcher;
pub mod write_echo;

pub use error::{Result, VaultError};
pub use folder::VaultFolder;
pub use note::{Frontmatter, Note, Wikilink};
pub use vault::Vault;
pub use watcher::{VaultEvent, VaultWatcher};
pub use write_echo::WriteEchoCache;
