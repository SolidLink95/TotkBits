use crate::{DocumentState::DocumentState, Open_and_Save::SendData};
use rfd::MessageDialog;
use tauri::Manager;

fn show_open_error(data: &SendData) {
    let message = if data.status_text.is_empty() {
        &data.text
    } else {
        &data.status_text
    };
    // The Mii request layer already shows a native connection dialog. Avoid
    // immediately presenting the same failure a second time in the open flow.
    if message.contains("Mii renderer request failed") {
        return;
    }
    MessageDialog::new()
        .set_title("TotkBits - Open error")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn report_monaco_save_error(tab: &str, result: &Option<SendData>, report_missing: bool) {
    if !matches!(tab.to_ascii_uppercase().as_str(), "TEXT" | "YAML") {
        return;
    }
    let message = match result {
        Some(data)
            if data.tab != "ERROR"
                && !data
                    .status_text
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("error") =>
        {
            return
        }
        Some(data) if !data.status_text.trim().is_empty() => data.status_text.clone(),
        Some(data) if !data.text.trim().is_empty() => data.text.clone(),
        None if !report_missing => return,
        _ => format!("Failed to save the {tab} document. No error details were returned."),
    };
    MessageDialog::new()
        .set_title("TotkBits - Save error")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

macro_rules! with_document_mut {
    ($handle:expr, $id:expr, $app:ident, $body:expr) => {{
        let documents = $handle.state::<DocumentState>();
        documents.with_mut(&$id, |$app| $body)
    }};
}

macro_rules! with_document {
    ($handle:expr, $id:expr, $app:ident, $body:expr) => {{
        let documents = $handle.state::<DocumentState>();
        documents.with(&$id, |$app| $body)
    }};
}

#[path = "commands/archives.rs"]
mod archives;
#[path = "commands/audio.rs"]
mod audio;
#[path = "commands/files.rs"]
mod files;
#[path = "commands/general.rs"]
mod general;
#[path = "commands/items_creator.rs"]
mod items_creator;
#[path = "commands/physics.rs"]
mod physics;
#[path = "commands/rstb.rs"]
mod rstb;
#[path = "commands/settings.rs"]
mod settings;
#[path = "commands/visuals.rs"]
pub(crate) mod visuals;

pub use archives::*;
pub use audio::*;
pub use files::*;
pub use general::*;
pub use items_creator::*;
pub use physics::*;
pub use rstb::*;
pub use settings::*;
pub use visuals::*;
