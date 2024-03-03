use monaco::{api::CodeEditorOptions, sys::editor::BuiltinTheme, yew::CodeEditor};
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::to_value;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

const CONTENT: &str = include_str!("test.yaml");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "tauri"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Serialize, Deserialize)]
struct GreetArgs<'a> {
    name: &'a str,
}

#[function_component(App)]
pub fn app() -> Html {
    let active_tab = use_state(|| 2); // Assuming the 2nd tab is shown by default

    let switch_to_tab = {
        let active_tab = active_tab.clone();
        move |tab: i32| {
            let active_tab = active_tab.clone();
            Callback::from(move |_: MouseEvent| active_tab.set(tab))
        }
    };
    let options = CodeEditorOptions::default()
        .with_language("json".to_owned())
        .with_value(CONTENT.to_owned())
        .with_builtin_theme(BuiltinTheme::VsDark)
        .with_automatic_layout(true);
    let greet_input_ref = use_node_ref();

    let name = use_state(|| String::new());

    let greet_msg = use_state(|| String::new());
    {
        let greet_msg = greet_msg.clone();
        let name = name.clone();
        let name2 = name.clone();
        use_effect_with(name2, move |_| {
            spawn_local(async move {
                if name.is_empty() {
                    return;
                }

                let args = to_value(&GreetArgs { name: &*name }).unwrap();
                // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
                let new_msg = invoke("greet", args).await.as_string().unwrap();
                greet_msg.set(new_msg);
            });

            || {}
        });
    }

    let greet = {
        let name = name.clone();
        let greet_input_ref = greet_input_ref.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            name.set(
                greet_input_ref
                    .cast::<web_sys::HtmlInputElement>()
                    .unwrap()
                    .value(),
            );
        })
    };

    html! {
        <main class="container">
            // Menu bar
            <div class="dropdown">
            <button class="dropbtn">{"Plik"}</button>
            <div class="dropdown-content">
                <a href="#">{"Nowy Ctrl+N"}</a>
                <a href="#">{"Zamknij Ctrl+W"}</a>
            </div>
        </div>

            // Additional bar with image buttons
            <div class="image-buttons-bar">
                <button><img src="path-to-image1.png" alt="Button 1"/></button>
                <button><img src="path-to-image2.png" alt="Button 2"/></button>
                <button><img src="path-to-image3.png" alt="Button 3"/></button>
            </div>
            <div style="background-color: #f2f2f2; padding: 0px 0; text-align:left;">
            <select style="padding: zpx; border-radius: 0;">
                <option value="option1">{"Option 1"}</option>
                <option value="option2">{"Option 2"}</option>
            </select>
        </div>
            <div class="image-buttons-bar">
            <CodeEditor classes={"full-height"} options={ options.to_sys_options() } />
            </div>

            // Tab content
            <div class="tab-content">
                {
                    if *active_tab == 1 {
                        html! { <div>{"Home Tab Content"}</div> }
                    } else if *active_tab == 2 {
                        html! {
                           
            <div>
            <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/vscode-codicons@0.0.17/dist/codicon.min.css"/>
            </div>
                        }
                    } else if *active_tab == 3 {
                        html! { <div>{"Settings Tab Content"}</div> }
                    } else {
                        html! {}
                    }
                }
            </div>

            // Footer
            <p>
                {"Recommended IDE setup: "}
                <a href="https://code.visualstudio.com/" target="_blank">{"VS Code"}</a>
                {" + "}
                <a href="https://github.com/tauri-apps/tauri-vscode" target="_blank">{"Tauri"}</a>
                {" + "}
                <a href="https://github.com/rust-lang/rust-analyzer" target="_blank">{"rust-analyzer"}</a>
            </p>

        </main>
    }
}
