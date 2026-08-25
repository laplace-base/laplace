#![allow(dead_code)]

use hypertext::prelude::{GlobalAttributes, hypertext_elements};
use hypertext::{Renderable, RenderableExt, rsx};
use iconic::fontawesome;
use wasm_bindgen::prelude::*;
use wingy_hypertext::appearance::Appearance::*;
use wingy_hypertext::attributes::CommonAttributeSetters;
use wingy_hypertext::class::{CLUSTER, GAP_XL, GAP_XS, GRID, HEADING_M, ICON, SIZE_SMALL, SPLIT, START};
use wingy_hypertext::component::button::Button;
use wingy_hypertext::layout::page::{Page, PageBody};
use wingy_hypertext::link::LinkSetters;
use wingy_hypertext::link::Target::*;
use wingy_hypertext::variant::Variant::*;
use wingy_hypertext_web::{init_page_element, listen_click_actions};

mod api;
mod lapps;

#[wasm_bindgen]
pub fn init() {
    console_log::init_with_level(log::Level::Debug).expect("could not initialize logger");

    init_page_element();
    listen_click_actions();

    lapps::register_actions();
}

#[wasm_bindgen]
pub fn route(path: &str) -> String {
    main_section(path).render().into_inner()
}

fn main_section(_route_path: &str) -> impl Renderable {
    lapps::list()
}

#[wasm_bindgen]
pub fn render_root(url_path: &str) -> String {
    rsx! {
        <Page>
            <header class=SPLIT>
                <div class=CLUSTER>
                    <span class=ICON style="color: var(--wa-color-brand-fill-loud); font-size: 1.5em; --rotate-angle: 0deg;">
                        (fontawesome::solid::PuzzlePiece)
                    </span>
                    <span id="brand-name" class=(HEADING_M, " ", "wa-desktop-only")>"Laplace"</span>
                </div>
                <div class=(CLUSTER, " ", GAP_XS)>
                    <Button variant=Brand appearance=Accent class=SIZE_SMALL href="https://github.com/laplace-base/laplace" target=Blank>
                        <span class=(START, " ", ICON)>
                            (fontawesome::brand::Github)
                        </span>
                        "GitHub"
                    </Button>
                </div>
            </header>
            <PageBody>
                <div></div>
                <main class="main-content">
                    (main_section(url_path))
                </main>
                <aside>
                </aside>
            </PageBody>
            <footer class=(GRID, " ", GAP_XL)>
                <div class=CLUSTER style="flex-wrap: nowrap">
                    <span class=ICON>
                        (fontawesome::solid::PuzzlePiece)
                    </span>
                    <span id="brand-name" class=HEADING_M>"Laplace"</span>
                </div>
            </footer>
        </Page>
    }
    .render()
    .into_inner()
}
