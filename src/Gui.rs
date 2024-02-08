use crate::misc::{open_file_dialog, save_file_dialog};
use eframe::egui::{
    self, Color32, ScrollArea, SelectableLabel, TextStyle, TextureId, TopBottomPanel,
};
use egui::emath::Numeric;
use egui::{CollapsingHeader, Context};
//use eframe::epi::App;
//use eframe::epi::Frame;
use egui::text::Fonts;
use image::io::Reader as ImageReader;
use native_dialog::{MessageDialog, MessageType};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{any, fs, io};

#[derive(PartialEq)]
enum ActiveTab {
    DiretoryTree,
    TextBox,
}

struct TotkBitsApp {
    opened_file: String,
    text: String,
    status_text: String,
    scroll: ScrollArea,
    active_tab: ActiveTab,
    language: String,
}
impl Default for TotkBitsApp {
    fn default() -> Self {
        Self {
            opened_file: String::new(),
            //text: "fn main() {\n                println!(\"Hello world!\");\n            }".to_owned(),
            text: "$parent: Work/Component/ArmorParam/Default.game__component__ArmorParam.gyml
ArmorEffect: []
BaseDefense: 3
HasSoundCloth: true
HeadEarBoneOffset: {X: 0.0,Y: 50.0,Z: 0.0}
HeadMantleType: DoubleMantle
HeadSwapActor: Work/Actor/Armor_001_Head_B.engine__actor__ActorParam.gyml
HiddenMaterialGroupList: []
HideMaterialGroupNameList: [G_Head,G_Scarf]
NextRankActor: Work/Actor/Armor_002_Head.engine__actor__ActorParam.gyml
SeriesName: Hylia
SoundMaterial: Cloth
WindEffectMesh: Mant_001_Havok
WindEffectScale: 0.3
            "
            .to_owned(),
            status_text: "Ready".to_owned(),
            scroll: ScrollArea::vertical(),
            active_tab: ActiveTab::TextBox,
            language: "toml".into(),
        }
    }
}

impl eframe::App for TotkBitsApp {
    //fn name(&self) -> &str {
    //    "Totkbits"
    //}

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top panel (menu bar)
        Gui::display_menu_bar(self, ctx);

        // Bottom panel (status bar)
        Gui::display_status_bar(self, ctx);
        // Central panel (text area)
        egui::CentralPanel::default().show(ctx, |ui| {
            Gui::display_labels(self, ui);

            Gui::display_main(self, ui);
        });
    }
}

struct Gui {}

impl Gui {
    pub fn display_status_bar(ob: &mut TotkBitsApp, ctx: &egui::Context) {
        TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&ob.status_text);
                // You can add more status information here
            });
        });
    }

    pub fn display_menu_bar(ob: &mut TotkBitsApp, ctx: &egui::Context) {
        //ob.open_icon_id = Gui::load_icon("res/open_icon.png");
        //ob.save_icon_id = Gui::load_icon("res/save_icon.png");
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    Gui::open_file_button_click(ob);
                }
                if ui.button("Save").clicked() {
                    // Logic for saving the current text
                    if ob.opened_file.len() > 0 {
                        println!("Saving file to {}", ob.opened_file);
                    }
                }
                if ui.button("Save as").clicked() {
                    // Logic for saving the current text
                    let file_path = save_file_dialog();
                    if file_path.len() > 0 {
                        println!("Saving file to {}", file_path);
                    }
                }

                // Add more menu items here
            });
        });
    }

    pub fn display_main(ob: &mut TotkBitsApp, ui: &mut egui::Ui) {
        let mut theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
        //ui.collapsing("Theme", |ui| {
        //    ui.group(|ui| {
        //        theme.ui(ui);
        //        theme.clone().store_in_memory(ui.ctx());
        //   })
        //});

        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job =
                egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, &ob.language);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };

        match ob.active_tab {
            ActiveTab::TextBox => {
                //scrollbar
                ob.scroll.clone().show(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        //egui::TextEdit::multiline(&mut ob.text).code_editor().desired_rows(10),
                        egui::TextEdit::multiline(&mut ob.text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(10)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter),
                    );
                });
            }
            ActiveTab::DiretoryTree => {
                //ui.allocate_space(ui.available_size());
                CollapsingHeader::new("Label1")
                    .default_open(false)
                    .show(ui, |ui| {
                        CollapsingHeader::new("Label2")
                            .default_open(false)
                            .show(ui, |ui| ui.label("ASDF"))
                    });

                let mut  paths = get_paths();
                //Gui::create_nested_ticks(ob, ui,paths);

                //ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLACK);
            }
        }
    }

    
 
    
        
    }

    pub fn display_labels(ob: &mut TotkBitsApp, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(SelectableLabel::new(
                    ob.active_tab == ActiveTab::DiretoryTree,
                    "Sarc files",
                ))
                .clicked()
            {
                ob.active_tab = ActiveTab::DiretoryTree;
            }
            if ui
                .add(SelectableLabel::new(
                    ob.active_tab == ActiveTab::TextBox,
                    "Byml editor",
                ))
                .clicked()
            {
                ob.active_tab = ActiveTab::TextBox;
            }
        });
    }

    fn open_file_button_click(ob: &mut TotkBitsApp) {
        // Logic for opening a file
        let file_name = open_file_dialog();
        if file_name.len() > 0 {
            ob.status_text = format!("Opened file: {}", file_name).to_owned();
            ob.opened_file = file_name.clone();
            let mut f_handle = fs::File::open(file_name.clone()).unwrap();
            let mut buffer: Vec<u8> = Vec::new(); //String::new();
            match f_handle.read_to_end(&mut buffer) {
                Ok(_) => ob.text = String::from_utf8_lossy(&buffer).to_string(),
                Err(err) => ob.status_text = format!("Error reading file: {}", file_name),
            }
            //self.text = buffer;
        } else {
            ob.status_text = "No file selected".to_owned();
        }
    }

    fn get_label_height(ctx: &egui::Context) -> f32 {
        //ctx.fonts() unused
        //    .layout_no_wrap("Example".to_string(), TextStyle::Heading, Color32::BLACK)
        //    .size()
        //    .y
        0.0
    }
}

pub fn get_paths() -> Vec<&'static str> {
    let s = "Component/ModelInfo/Armor_006_Head.engine__component__ModelInfo.bgyml
GameParameter/GameParameterTable/Armor_006_Upper.engine__actor__GameParameterTable.bgyml
GameParameter/EnhancementMaterial/Armor_006_Head.game__pouchcontent__EnhancementMaterial.bgyml
Component/ActorPositionCalculatorParam/Armor_006_Upper.game__component__ActorPositionCalculatorParam.bgyml
Component/CaptureParam/Armor_006_Upper.game__component__CaptureParam.bgyml
Component/ArmorParam/Armor_006_Upper.game__component__ArmorParam.bgyml
GameParameter/PriceParam/Armor_006_Upper.game__pouchcontent__PriceParam.bgyml
Phive/HelperBone/Armor_006_RaulSkin_Upper.bphhb
Phive/Cloth/Armor_006_RaulSkin_Upper.bphcl
Component/ColorVariationParam/Armor_Upper.game__component__ColorVariationParam.bgyml
Component/ASInfo/Upper_Common.engine__component__ASInfo.bgyml";
    let paths: Vec<&str> = s.split("\n").collect();
    return paths;
}

pub fn run() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Totkbits",
        options,
        Box::new(|_cc| Box::<TotkBitsApp>::default()),
    )
    .unwrap();
}
