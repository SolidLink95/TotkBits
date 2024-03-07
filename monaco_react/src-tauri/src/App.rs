use std::sync::Arc;

use crate::file_format::Pack::PackComparer;
use crate::TotkConfig::TotkConfig;
use crate::{file_format::BinTextFile::OpenedFile};
use crate::Zstd::TotkZstd;

#[derive(PartialEq)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
    Settings,
}


pub struct TotkBitsApp<'a> {
    pub opened_file: OpenedFile<'a>,     //path to opened file in string
    pub text: String,
    pub status_text: String,
    pub active_tab: ActiveTab,           //active tab, either sarc file or text editor
    pub zstd: Arc<TotkZstd<'a>>,
    pub pack: Option<PackComparer<'a>>,

}

impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        let totk_config: Arc<TotkConfig> = Arc::new(TotkConfig::new());
        let zstd: Arc<TotkZstd<'_>> = Arc::new(TotkZstd::new(totk_config, 16).unwrap());
        Self {
            opened_file: OpenedFile::default(),
            text: "".to_string(),
            status_text: "".to_string(),
            active_tab: ActiveTab::DiretoryTree,
            zstd: zstd.clone(),
            pack: None,
        }
    }
}

impl<'a> TotkBitsApp<'_> {}