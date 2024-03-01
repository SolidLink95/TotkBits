use egui::{Align, Label, Layout, SelectableLabel};

use crate::Gui::{ActiveTab, TotkBitsApp};

pub struct InfoLabel {
    label: String,
    value: String,
}

impl InfoLabel {
  

    pub fn display_filename_endian(app: &mut TotkBitsApp, ui: &mut egui::Ui) {
        match app.active_tab {
            ActiveTab::DiretoryTree => {
                if let Some(pack) = &app.pack {
                    if let Some(opened) = &pack.opened {
                        let label_endian = match opened.endian {
                            roead::Endian::Big => "BE",
                            roead::Endian::Little => "LE",
                        };
                        Self::display_infolabels(ui, label_endian, Some(&opened.path.name));
                    }
                }
            }
            ActiveTab::TextBox => {
                let mut label_path: Option<&str> = None;
                let label_endian = &app.opened_file.get_endian_label();
                if let Some(internal_file) = &app.internal_sarc_file {
                    label_path = Some(&internal_file.path.name);
                } else {
                    label_path = Some(&app.opened_file.path.name);
                }

                Self::display_infolabels(ui, label_endian, label_path);
            }
            ActiveTab::Settings => {
                Self::display_infolabels(ui, "", Some(""));
            }
        }
    }

    //surprise me
    fn calc_labels_width(label: &str) -> f32 {
        (label.len() + 3) as f32 * 6.0 //very rough calculation based on default Style
    }

    fn are_infolables_shown(ui: &mut egui::Ui, label: &str) -> bool {
        let perc = Self::calc_labels_width(label) / ui.available_width();
        return perc < 0.79;
    }
    pub fn display_infolabels(ui: &mut egui::Ui, endian: &str, path: Option<&str>) {
        if let Some(path) = &path {
            if Self::are_infolables_shown(ui, path) {
                //ui.add(Label::new(endian));
                ui.add(Label::new(format!("{} {}", path, endian)));
                //ui.add(Label::new(path.to_string()));
            }
        }
    }
}
