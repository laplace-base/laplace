//! The Todo lapp frontend, after TodoMVC: a new task, the list with marks, in-place editing and
//! removal, filters, marking all and clearing the completed ones.
//!
//! Every action is an htmx request to `/todo/api/<action>` that the `client-router` extension hands to
//! [`router`] with the values of the form; the router calls the back through `handle_http` and
//! renders the whole `#todo` section anew.

use std::cell::Cell;
use std::collections::HashMap;

use hypertext::prelude::{GlobalAttributes, HtmxAttributes, hypertext_elements};
use hypertext::{Renderable, RenderableExt, rsx};
use iconic::fontawesome;
use laplace_front_api::http::{Request, handle_http};
use todo_common::{Response, Task};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_dom::UnwrapThrowExt;
use wingy_hypertext::appearance::Appearance::*;
use wingy_hypertext::attributes::CommonAttributeSetters;
use wingy_hypertext::class::{CLUSTER, GAP_XS, HEADING_M, ICON, SIZE_SMALL, SPLIT, STACK, START};
use wingy_hypertext::component::button::Button;
use wingy_hypertext::component::callout::Callout;
use wingy_hypertext::component::checkbox::Checkbox;
use wingy_hypertext::component::input::Input;
use wingy_hypertext::htmx::HtmxSetters;
use wingy_hypertext::layout::page::{Page, PageBody};
use wingy_hypertext::link::LinkSetters;
use wingy_hypertext::link::Target::*;
use wingy_hypertext::variant::Variant::*;
use wingy_hypertext_web::init_page_element;

const API: &str = "/todo/api";

const TODO: &str = "todo";
const TODO_LIST: &str = "todo-list";
const TODO_ITEM: &str = "todo-item";
const COMPLETED: &str = "completed";
const DESCRIPTION: &str = "description";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Active,
    Completed,
}

impl Filter {
    const ALL: [Self; 3] = [Self::All, Self::Active, Self::Completed];

    fn fits(self, task: &Task) -> bool {
        match self {
            Self::All => true,
            Self::Active => !task.completed,
            Self::Completed => task.completed,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Active => "active",
            Self::Completed => "completed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Active => "Active",
            Self::Completed => "Completed",
        }
    }

    fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|filter| filter.name() == name)
            .unwrap_or(Self::All)
    }
}

thread_local! {
    /// The filter of the list: a state of the window, not of the tasks.
    static FILTER: Cell<Filter> = const { Cell::new(Filter::All) };
}

#[wasm_bindgen]
pub fn init() {
    console_log::init_with_level(log::Level::Debug).expect_throw("could not initialize logger");
    init_page_element();
}

/// Handles the htmx request `path` with the form values `params` (a JSON object) and renders the
/// `#todo` section; an error of the action is shown above the list as it is after it.
#[wasm_bindgen]
pub async fn router(path: &str, params: &str) -> String {
    let params: HashMap<String, String> = serde_json::from_str(params).unwrap_or_default();
    let action = path.strip_prefix(API).unwrap_or(path);
    log::debug!("action {action}: {params:?}");

    let (tasks, error) = match act(action, &params).await {
        Ok(tasks) => (tasks, None),
        Err(err) => {
            log::error!("{action}: {err}");
            (list().await.unwrap_or_default(), Some(err))
        },
    };

    todo_section(&tasks, FILTER.get(), error.as_deref())
        .render()
        .into_inner()
}

async fn act(action: &str, params: &HashMap<String, String>) -> Result<Vec<Task>, String> {
    let param = |name: &str| params.get(name).map(String::as_str).unwrap_or_default();
    let id = || {
        param("id")
            .parse::<i64>()
            .map_err(|err| format!("Invalid task id: {err}"))
    };

    match action {
        "/view" => list().await,
        "/add" => {
            let description = param(DESCRIPTION).trim();
            if description.is_empty() {
                return list().await;
            }
            let task = Task {
                description: description.into(),
                ..Task::default()
            };
            call("POST", "/add", json(&task)?).await
        },
        // The checkbox is in the form only when it is checked.
        "/toggle" => {
            let task = Task {
                id: id()?,
                description: param(DESCRIPTION).into(),
                completed: params.contains_key(COMPLETED),
            };
            call("POST", "/update", json(&task)?).await
        },
        // An emptied description removes the task, as in TodoMVC.
        "/update" => {
            let description = param(DESCRIPTION).trim();
            if description.is_empty() {
                return call("POST", "/delete", id()?.to_string()).await;
            }
            let task = Task {
                id: id()?,
                description: description.into(),
                completed: param(COMPLETED) == "true",
            };
            call("POST", "/update", json(&task)?).await
        },
        "/delete" => call("POST", "/delete", id()?.to_string()).await,
        "/toggle_all" => {
            let filter = FILTER.get();
            let mut tasks = list().await?;
            let visible: Vec<_> = tasks.iter().filter(|task| filter.fits(task)).collect();
            let completed = !visible.is_empty() && !visible.iter().all(|task| task.completed);

            for task in tasks
                .iter_mut()
                .filter(|task| filter.fits(task) && task.completed != completed)
            {
                task.completed = completed;
                call("POST", "/update", json(task)?).await?;
            }
            list().await
        },
        "/clear_completed" => call("POST", "/clear_completed", String::new()).await,
        "/filter" => {
            FILTER.set(Filter::from_name(param("filter")));
            list().await
        },
        action => Err(format!("Unknown action '{action}'")),
    }
}

async fn list() -> Result<Vec<Task>, String> {
    call("GET", "/list", String::new()).await
}

/// Calls the back: every action of it answers with the tasks after it or with an error.
async fn call(method: &str, path: &str, body: String) -> Result<Vec<Task>, String> {
    let request = Request::builder()
        .method(method)
        .uri(format!("{API}{path}"))
        .body(body.into_bytes())
        .build()
        .map_err(|err| err.to_string())?;
    let response = handle_http(&request)
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;

    match serde_json::from_slice(&response.body) {
        Ok(Response::List(tasks)) => Ok(tasks),
        Ok(Response::Error(err)) => Err(err),
        Err(err) => Err(format!("Invalid response of the back ({}): {err}", response.status)),
    }
}

fn json(task: &Task) -> Result<String, String> {
    serde_json::to_string(task).map_err(|err| err.to_string())
}

/// The attributes of every request of the section: it is replaced with the answer.
const TARGET: &str = "#todo";
const SWAP: &str = "outerHTML";

fn todo_section(tasks: &[Task], filter: Filter, error: Option<&str>) -> impl Renderable {
    let visible: Vec<_> = tasks.iter().filter(|task| filter.fits(task)).collect();
    let left = tasks.iter().filter(|task| !task.completed).count();
    let completed = tasks.len() - left;

    rsx! {
        <section id=TODO class=(STACK, " ", TODO)>
            <form class=CLUSTER hx-post=(format!("{API}/add")) hx-target=TARGET hx-swap=SWAP>
                <Input class=DESCRIPTION name=DESCRIPTION placeholder="What needs to be done?" autofocus=true />
                <Button variant=Brand>"Add"</Button>
            </form>
            @if let Some(error) = error {
                <Callout variant=Danger>(error)</Callout>
            }
            <ul class=(STACK, " ", GAP_XS, " ", TODO_LIST)>
                @for task in &visible {
                    (task_item(task))
                }
            </ul>
            @if !tasks.is_empty() {
                <footer class=SPLIT>
                    <span>(left) @if left == 1 { " item left" } @else { " items left" }</span>
                    <div class=(CLUSTER, " ", GAP_XS)>
                        @for option in Filter::ALL {
                            <Button
                                class=SIZE_SMALL
                                appearance=(if option == filter { Accent } else { Plain })
                                hx_post=(format!("{API}/filter"))
                                hx_vals=(format!(r#"{{"filter":"{}"}}"#, option.name()))
                                hx_target=TARGET
                                hx_swap=SWAP
                            >
                                (option.label())
                            </Button>
                        }
                    </div>
                    <div class=(CLUSTER, " ", GAP_XS)>
                        <Button class=SIZE_SMALL appearance=Outlined hx_post=(format!("{API}/toggle_all")) hx_target=TARGET hx_swap=SWAP>
                            "Mark all"
                        </Button>
                        <Button
                            class=SIZE_SMALL
                            appearance=Outlined
                            disabled=(completed == 0)
                            hx_post=(format!("{API}/clear_completed"))
                            hx_target=TARGET
                            hx_swap=SWAP
                        >
                            "Clear completed"
                        </Button>
                    </div>
                </footer>
            }
        </section>
    }
}

fn task_item(task: &Task) -> impl Renderable {
    let item_class = if task.completed {
        format!("{CLUSTER} {TODO_ITEM} {COMPLETED}")
    } else {
        format!("{CLUSTER} {TODO_ITEM}")
    };

    rsx! {
        <li class=(item_class)>
            <form hx-post=(format!("{API}/toggle")) hx-trigger="change" hx-target=TARGET hx-swap=SWAP>
                <input type="hidden" name="id" value=(task.id) />
                <input type="hidden" name=DESCRIPTION value=(&task.description) />
                <Checkbox name=COMPLETED value="true" checked=(task.completed) />
            </form>
            <form class=DESCRIPTION hx-post=(format!("{API}/update")) hx-trigger="change, submit" hx-target=TARGET hx-swap=SWAP>
                <input type="hidden" name="id" value=(task.id) />
                <input type="hidden" name=COMPLETED value=(task.completed) />
                <Input appearance=Plain name=DESCRIPTION value=(&task.description) />
            </form>
            <Button
                class=SIZE_SMALL
                appearance=Plain
                variant=Danger
                hx_post=(format!("{API}/delete"))
                hx_vals=(format!(r#"{{"id":"{}"}}"#, task.id))
                hx_target=TARGET
                hx_swap=SWAP
            >
                <span class=ICON>(fontawesome::solid::Xmark)</span>
            </Button>
        </li>
    }
}

#[wasm_bindgen]
pub fn render_root() -> String {
    rsx! {
        <Page>
            <header class=SPLIT>
                <div class=CLUSTER>
                    <span class=ICON style="color: var(--wa-color-brand-fill-loud); font-size: 1.5em; --rotate-angle: 0deg;">
                        (fontawesome::solid::CircleCheck)
                    </span>
                    <span id="brand-name" class=(HEADING_M, " ", "wa-desktop-only")>"Todo example"</span>
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
                    <div id=TODO class=TODO hx-post=(format!("{API}/view")) hx-trigger="load" hx-target="this" hx-swap=SWAP></div>
                </main>
                <aside>
                </aside>
            </PageBody>
        </Page>
    }
    .render()
    .into_inner()
}
