use hypertext::prelude::{GlobalAttributes, hypertext_elements};
use hypertext::{Renderable, RenderableExt, rsx};
use iconic::fontawesome;
use laplace_front_api::http::{Request, handle_http};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_dom::{self as dom, UnwrapThrowExt};
use web_sys::HtmlInputElement;
use wingy_hypertext::appearance::Appearance::*;
use wingy_hypertext::attributes::CommonAttributeSetters;
use wingy_hypertext::attrs;
use wingy_hypertext::class::{CLUSTER, GAP_XS, HEADING_M, ICON, SIZE_SMALL, SPLIT, STACK, START};
use wingy_hypertext::component::button::Button;
use wingy_hypertext::component::input::Input;
use wingy_hypertext::layout::page::{Page, PageBody};
use wingy_hypertext::link::LinkSetters;
use wingy_hypertext::link::Target::*;
use wingy_hypertext::variant::Variant::*;
use wingy_hypertext_web::init_page_element;

#[wasm_bindgen]
pub fn init() {
    console_log::init_with_level(log::Level::Debug).expect_throw("could not initialize logger");
    init_page_element();
}

#[wasm_bindgen]
pub async fn router(path: &str) -> String {
    let path = if path == "/echo/api/" {
        let value = dom::existing::select_element_cast::<HtmlInputElement>("#uri-path input").value();
        format!("/echo/api/{}", value)
    } else {
        path.into()
    };

    log::info!("req: {path}");

    let request = Request::get(path).build().expect_throw("Error building request");
    let handle_http_fut = handle_http(&request).expect_throw("Error making handle_http");
    let response = handle_http_fut.await.expect_throw("Error calling handle_http");
    let response = String::from_utf8(response.body).expect_throw("Error decoding response body");

    log::info!("resp: {response}");

    rsx! { <div>(response)</div> }.render().into_inner()
}

fn main_section() -> impl Renderable {
    rsx! {
        <div class = STACK>
            <div class=CLUSTER>
                <Input id="uri-path" placeholder="URI path" required=true />
                <Button
                    attrs=(attrs!(
                        "hx-get" = &"/echo/api/",
                        "hx-target" = &"#responses-list",
                        "hx-swap" = &"afterbegin"
                    ))
                >
                    "Submit"
                </Button>
            </div>
            <div id = "responses-list" class = STACK>
            </div>
        </div>
    }
}

#[wasm_bindgen]
pub fn render_root() -> String {
    rsx! {
        <Page>
            <header class=SPLIT>
                <div class=CLUSTER>
                    <span class=ICON style="color: var(--wa-color-brand-fill-loud); font-size: 1.5em; --rotate-angle: 0deg;">
                        (fontawesome::solid::House)
                    </span>
                    <span id="brand-name" class=(HEADING_M, " ", "wa-desktop-only")>"Echo example"</span>
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
                    (main_section())
                </main>
                <aside>
                </aside>
            </PageBody>
        </Page>
    }
    .render()
    .into_inner()
}
