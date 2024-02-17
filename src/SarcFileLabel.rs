use std::io;
use std::rc::Rc;

use crate::BinTextFile::{BymlFile, FileData};
use msyt::converter::MsytFile;

use crate::Gui::{ActiveTab, OpenedFile, TotkBitsApp};
use crate::Tree::TreeNode;
use crate::Zstd::{is_aamp, is_byml, is_msyt, FileType};

use egui::{
    SelectableLabel,
};
use egui::{CollapsingHeader};
use roead::byml::Byml;

pub struct SarcLabel {
    //root_node: Rc<TreeNode<String>>,
    //node: &'a Rc<TreeNode<String>>,
    //app: &'a mut TotkBitsApp<'a>,
    //ui: &'a egui::Ui,
}

impl SarcLabel {
    pub fn new() -> Self {
        Self {} // { node, app }
    }

    fn display_leaf_node(
        app: &mut TotkBitsApp,
        child: &Rc<TreeNode<String>>,
        ui: &mut egui::Ui,
    ) {
        ui.horizontal(|ui| {
            let file_label = ui.add(SelectableLabel::new(
                SarcLabel::is_internal_file_selected(app, child),
                &child.value,
            ));
            if file_label.double_clicked() {
                //println!("Clicked {}", child.full_path.clone());
                app.internal_sarc_file = Some(child.clone());
                SarcLabel::safe_open_file_from_opened_sarc(
                    app,
                    ui,
                    &child,
                )
            }
            if file_label.clicked() {
                //println!("Double Clicked {}", child.full_path.clone());
                app.internal_sarc_file = Some(child.clone());
            }
            if file_label.secondary_clicked() {
                println!("Mocking future context menu for {}", &child.path.full_path);
            }
            file_label.context_menu(|ui|{
                if ui.button("Button 1").clicked() {
                    // Handle Button 1 click
                    println!("Button 1 clicked");
                    ui.close_menu();
                }

                if ui.button("Button 2").clicked() {
                    // Handle Button 2 click
                    println!("Button 2 clicked");
                    ui.close_menu();
                }
            } );
        });
    }

    pub fn display_tree_in_egui(
        app: &mut TotkBitsApp,
        root_node: &Rc<TreeNode<String>>,
        ui: &mut egui::Ui,
    ) {
        
        if TreeNode::is_leaf(&root_node) { //rarely files in sarc are in root directory
            SarcLabel::display_leaf_node(app, root_node, ui);
            return;
        }
        let response = CollapsingHeader::new(root_node.value.clone())
            .default_open(false)
            .show(ui, |ui| {
                for child in root_node.children.borrow().iter() {
                    if !TreeNode::is_leaf(&child) {
                        SarcLabel::display_tree_in_egui(app, child, ui);
                    } else {
                        SarcLabel::display_leaf_node(app, child, ui);
                    }
                }
            });
        //TODO: custom collapsing header (ui.horizontal with image and selectablelabel)
        if response.header_response.secondary_clicked() {
            println!("Mock for context menu {}",&root_node.path.full_path);
        }
    }

    pub fn is_internal_file_selected(app: &mut TotkBitsApp, child: &Rc<TreeNode<String>>) -> bool {
        match &app.internal_sarc_file {
            Some(x) => {
                if x.path.full_path == child.path.full_path {
                    return true;
                }
                return false;
            }
            None => {
                return false;
            }
        }
    }

    pub fn safe_open_file_from_opened_sarc(app: &mut TotkBitsApp, ui: &mut egui::Ui, child: &Rc<TreeNode<String>>) {
        match SarcLabel::open_file_from_opened_sarc(app, ui, child) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("Failed to open {}, \nError: {:?}", &child.path.full_path.clone(), err);
                app.status_text = format!("Failed to open {}", &child.path.full_path.clone());
            }
        }
    }

    fn open_file_from_opened_sarc(app: &mut TotkBitsApp, _ui: &mut egui::Ui, child: &Rc<TreeNode<String>>) -> io::Result<()> {
        if app.pack.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "No sarc opened"));
        }
        let path = &child.path.full_path.clone();
        let op_sarc = app.pack.as_ref().unwrap();
        let data = op_sarc.sarc.get_data(&path);
        if data.is_none() {
            return Err(io::Error::new(io::ErrorKind::Other, "File absent in sarc"));
        }
        //For now assume only byml and msyt files will be opened
        let raw_data = data.unwrap().to_vec();
        if is_msyt(&raw_data) {
            app.text = MsytFile::binary_to_text(raw_data).expect("Error getting file msyt");
            app.opened_file = OpenedFile::new(path.to_string(), FileType::Msbt, Some(roead::Endian::Little), None);
            app.settings.is_file_loaded = true; //precaution
        }
        else if is_byml(&raw_data) {
            let file_data = FileData::from(raw_data,FileType::Byml);
            let the_byml = BymlFile::from_binary(file_data, app.zstd.clone(), child.path.full_path.clone())?;
            app.opened_file = OpenedFile::new(path.to_string(), FileType::Byml,  Some(roead::Endian::Little), None);
            app.text = Byml::to_text(&the_byml.pio);
            app.opened_file.byml = Some(the_byml);
            app.settings.is_file_loaded = true; //precaution
        }
        else if is_aamp(&raw_data) {
            //placeholder for aamp
        }
        if app.settings.is_file_loaded { //something got opened
            app.internal_sarc_file = Some(child.clone());
            app.active_tab = ActiveTab::TextBox;
            app.status_text = format!("Opened: {}", path);

        }
  
        Ok(())
    }
}
