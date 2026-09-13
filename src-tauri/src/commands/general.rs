use crate::{DocumentState::DocumentState, Open_and_Save::SendData};
use reqwest::blocking::Client;
use tauri::Manager;

#[tauri::command]
pub fn search_in_sarc(
    app_handle: tauri::AppHandle,
    documentId: String,
    query: String,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document_mut!(app_handle, documentId, app, app.search_in_sarc(query)),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn clear_search_in_sarc(app_handle: tauri::AppHandle, documentId: String) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document_mut!(app_handle, documentId, app, app.clear_search_in_sarc()),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

//COMPARE stuff
#[tauri::command]
pub fn compare_files(
    app_handle: tauri::AppHandle,
    documentId: String,
    isFromDisk: bool,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || with_document!(app_handle, documentId, app, app.compare_files(isFromDisk)),
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn compare_internal_file_with_vanila(
    app_handle: tauri::AppHandle,
    documentId: String,
    internal_path: String,
    is_from_sarc: bool,
) -> Option<SendData> {
    crate::Settings::catch_panic_with(
        move || {
            app_handle.state::<DocumentState>().compare_internal_file(
                &documentId,
                internal_path,
                is_from_sarc,
            )
        },
        |message| Some(crate::Open_and_Save::SendData::panicked(message)),
    )
}

#[tauri::command]
pub fn check_if_update_needed() -> String {
    crate::Settings::catch_panic_with(
        move || {
            let repo_owner = "SolidLink95".to_string();
            let repo_name = "TotkBits".to_string();
            let url = format!(
                "https://api.github.com/repos/{}/{}/releases/latest",
                repo_owner, repo_name
            );
            println!("Checking for updates...");
            let client = Client::new();
            let response = client.get(&url).header("User-Agent", "MyAppName").send();

            if let Ok(response) = response {
                // println!("Response: {:?}", response);

                if let Ok(json_value) = response.json::<serde_json::Value>() {
                    // println!("\n\nJson value: {:?}", json_value);
                    if let Some(release_info) = json_value["tag_name"].as_str() {
                        // println!("\n\nRelease info: {}", release_info);
                        let installed_ver = parse_release_version(env!("CARGO_PKG_VERSION"));
                        let latest_ver = parse_release_version(release_info);
                        if latest_ver.is_some() && latest_ver > installed_ver {
                            return release_info.to_string();
                        }
                    }
                }
            }
            String::new()
        },
        |message| format!("error: {message}"),
    )
}

fn parse_release_version(version: &str) -> Option<(u32, u32, u32)> {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let version = version.split_once('-').map_or(version, |(core, _)| core);
    let mut parts = version.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

#[cfg(test)]
mod update_check_tests {
    use super::parse_release_version;

    #[test]
    fn parses_github_release_tags() {
        assert_eq!(parse_release_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_release_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_release_version("not-a-version"), None);
    }
}
