import init, * as wasm from "/todo/dist/todo-front.js";
import define_client_router_ext from '/vendor/htmx/client-router.ext.js';

await init();

// The values of the form or of `hx-vals` of the element go to the router as a JSON object.
async function router(path, event) {
    const params = event.detail.formData
        ? Object.fromEntries(event.detail.formData)
        : { ...event.detail.parameters };
    return await wasm.router(path, JSON.stringify(params));
}
define_client_router_ext(htmx, "/todo/api", null, router);

let root_html = wasm.render_root();
let html_fragment = document.createRange().createContextualFragment(root_html);
let root = document.getElementById('root');
root.insertBefore(html_fragment, root.firstChild);

htmx.process(root);

wasm.init();
