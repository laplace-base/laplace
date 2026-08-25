export function define_client_router_ext(htmx, client_api, server_api, router, preproc) {
    htmx.defineExtension('client-router', {
        onEvent: function (name, event) {
            if (router && name === "htmx:configRequest") {
                let path = event.detail.path;

                if (client_api && path.startsWith(client_api) || server_api && !path.startsWith(server_api)) {
                    event.preventDefault();

                    let target = event.detail.target;
                    let swap_spec = event.detail.elt.getAttribute('hx-swap') || 'innerHTML';

                    Promise.resolve(router(path, event))
                        .then(html => htmx.swap(target, html, { swapStyle: swap_spec }))
                        .catch(err => console.error('client-router failed:', path, err));
                }
            } else if (preproc && name === "htmx:beforeSwap") {
                event.detail.serverResponse = preproc(event.detail.serverResponse, event);
            }
        }
    });
}

export { define_client_router_ext as default };
