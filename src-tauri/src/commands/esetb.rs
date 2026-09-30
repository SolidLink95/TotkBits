//! ESETB colour editor: the GUI edits the emitter colours of the PTCL
//! document embedded in the YAML text. Both commands are stateless: the text
//! of the Monaco editor stays the single source of truth for the save path.
use crate::file_format::Esetb::{ptcl_document_from_text, text_with_ptcl_document};
use crate::parser::ptcl::PtclDocument;

/// The emitter sets of the ESETB text (`PTCL_JSON`): set name → emitter name
/// → const colours and animations.
#[tauri::command]
pub fn esetb_read_colors(text: String) -> Result<PtclDocument, String> {
    crate::Settings::catch_panic_with(
        move || ptcl_document_from_text(&text).map_err(|e| e.to_string()),
        Err,
    )
}

/// The ESETB text with its `PTCL_JSON` node replaced by `document`.
#[tauri::command]
pub fn esetb_apply_colors(text: String, document: PtclDocument) -> Result<String, String> {
    crate::Settings::catch_panic_with(
        move || text_with_ptcl_document(&text, &document).map_err(|e| e.to_string()),
        Err,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_format::Esetb::Esetb;
    use crate::parser::ptcl::Ptcl;
    use std::{path::Path, sync::Arc};

    /// A colour picked in the GUI reaches the PTCL blob through the same text
    /// the YAML editor saves; untouched documents leave the text as it was.
    #[test]
    fn colour_edit_round_trips_through_the_yaml_text() {
        let config = crate::TotkConfig::TotkConfig::new(false).expect("TotkBits config");
        let path =
            Path::new(&config.romfs).join("Effect/Weapon_Lsword_002.Nin_NX_NVN.esetb.byml.zs");
        if !path.is_file() {
            eprintln!("skipped: no TOTK RomFS with {}", path.display());
            return;
        }
        let zstd = Arc::new(
            crate::Zstd::TotkZstd::new(Arc::new(config), crate::Zstd::TOTK_ZSTD_COMPRESSION_LEVEL)
                .expect("TotK Zstandard dictionaries"),
        );
        let (mut opened, data) = Esetb::open_esetb(&path, zstd.clone()).expect("opened ESETB");
        let text = data.text;
        let document = ptcl_document_from_text(&text).expect("PTCL document");
        assert!(!document.0.is_empty());
        assert_eq!(
            text_with_ptcl_document(&text, &document).expect("unchanged text"),
            text
        );

        let mut edited = document.clone();
        let (set_name, emitter_name) = edited
            .0
            .iter()
            .find_map(|(set, emitters)| {
                emitters
                    .keys()
                    .next()
                    .map(|name| (set.clone(), name.clone()))
            })
            .expect("an emitter");
        let emitter = edited
            .0
            .get_mut(&set_name)
            .unwrap()
            .get_mut(&emitter_name)
            .unwrap();
        emitter.const_color0 = [0.125, 0.25, 2.5, 1.0];
        emitter.color_anim0[0].value = [0.75, 0.5, 0.25];
        let edited_text = text_with_ptcl_document(&text, &edited).expect("edited text");
        assert_ne!(edited_text, text);
        assert_eq!(
            ptcl_document_from_text(&edited_text).expect("reread"),
            edited
        );

        let esetb = opened.esetb.as_mut().expect("esetb state");
        let binary = esetb.text_to_binary(&edited_text).expect("saved binary");
        let reopened = Esetb::from_binary(&binary, zstd).expect("reopened ESETB");
        let reparsed = Ptcl::parse(&reopened.ptcl).expect("saved PTCL");
        assert_eq!(reparsed.document, edited);
        assert_eq!(
            esetb_apply_colors(text.clone(), edited.clone()).expect("command"),
            edited_text
        );
        assert_eq!(esetb_read_colors(edited_text).expect("command"), edited);
        assert!(esetb_read_colors("not: [yaml".to_owned()).is_err());
    }
}
