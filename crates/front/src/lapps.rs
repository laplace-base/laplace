use hypertext::prelude::GlobalAttributes;
use hypertext::validation::hypertext_elements;
use hypertext::{Renderable, rsx};
use wasm_bindgen_futures::spawn_local;
use wasm_dom::correct::JsObjectAccess;
use wingy_hypertext::action::ActionSetters;
use wingy_hypertext::appearance::Appearance::*;
use wingy_hypertext::attributes::CommonAttributeSetters;
use wingy_hypertext::class::SIZE_SMALL;
use wingy_hypertext::component::button::Button;
use wingy_hypertext::link::LinkSetters;
use wingy_hypertext_web::util::action::register_action;

use crate::api;

pub fn register_actions() {
    register_action("open_lapp", |args, _ctx| {
        let lapp_name = args.get("name").as_string().unwrap_or_default();

        spawn_local(async move {
            if let Err(err) = api::open_lapp(&lapp_name).await {
                log::error!("Failed to open lapp '{lapp_name}': {err:?}");
            }
        });
    });
}

pub fn list() -> impl Renderable {
    let lapps = vec![Lapp {
        name: "echo".to_string(),
        enabled: true,
        title: "Echo".to_string(),
        autoload: true,
        permissions: Permissions {
            allowed: vec!["test".to_string()],
            required: vec!["test".to_string(), "prod".to_string()],
        },
    }];

    rsx! {
        <div class = "content-container">
            <h2 class = "title">"Applications"</h2>
            <div class = "lapps-table">
                @for lapp in &lapps {
                    (lapp_item(lapp))
                }
            </div>
        </div>
    }
}

pub fn lapp_item(lapp: &Lapp) -> impl Renderable {
    rsx! {
        <div class = "lapps-table-row">
            <div class = "lapps-table-col">
                <h3>
                    <Button class=SIZE_SMALL appearance=Outlined href="#" action="open_lapp" action_args=(format!(r#"{{"name":"{}"}}"#, lapp.name))>
                        (lapp.title)
                    </Button>
                </h3>
            </div>
        </div>
        <br />
    }
}

#[derive(Clone)]
pub struct Lapp {
    pub name: String,
    pub enabled: bool,
    pub title: String,
    pub autoload: bool,
    pub permissions: Permissions,
}

#[derive(Clone)]
pub struct Permissions {
    pub required: Vec<String>,
    pub allowed: Vec<String>,
}
