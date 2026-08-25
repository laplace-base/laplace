import init, * as wasm from "/echo/dist/echo-front.js";
import define_client_router_ext from '/vendor/htmx/client-router.ext.js';

await init();

async function router(path, event) {
    return await wasm.router(path);
}
define_client_router_ext(htmx, "/echo/api", null, router);

let root_html = wasm.render_root(window.location.pathname);
let html_fragment = document.createRange().createContextualFragment(root_html);
let root = document.getElementById('root');
root.insertBefore(html_fragment, root.firstChild);

htmx.process(root);

wasm.init();
