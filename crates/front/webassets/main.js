import init, * as wasm from "./dist/laplace-front.js";
import define_client_router_ext from '/vendor/htmx/client-router.ext.js';

await init();

function router(path, event) {
    return wasm.route(path);
}
define_client_router_ext(htmx, null, "/api", router);

let root_html = wasm.render_root(window.location.pathname);
let html_fragment = document.createRange().createContextualFragment(root_html);
let root = document.getElementById('root');
root.insertBefore(html_fragment, root.firstChild);

htmx.process(root);

wasm.init();
