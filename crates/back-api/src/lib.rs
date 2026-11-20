//! The SDK of a lapp backend: the bindings of the `laplace:lapp` WIT package (`wit/`), the `http`
//! types in place of the WIT ones, and a router by path.
//!
//! ```ignore
//! struct Echo;
//!
//! impl laplace_back_api::Lapp for Echo {
//!     fn handle(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
//!         Router::new().get("/hello", hello).handle(request)
//!     }
//! }
//!
//! laplace_back_api::export!(Echo);
//! ```
//!
//! A backend is a `cdylib` built for `wasm32-wasip2`. It always exports `lifecycle`: a lapp without
//! its own `init` gets the empty one.

pub use http;

pub use self::router::{Handler, Router, api_path};

pub mod convert;
pub mod database;
pub mod http_client;
pub mod router;

#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "back-init",
        pub_export_macro: true,
        export_macro_name: "export_bindings",
        default_bindings_module: "laplace_back_api::bindings",
    });
}

/// A lapp backend.
pub trait Lapp {
    /// Answers a request of the lapp frontend.
    fn handle(request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>>;

    /// Prepares the backend: runs on every new instance, before its first request, so it should be
    /// idempotent. `previous_version` is the version the lapp last ran with, if it was another one.
    fn init(_previous_version: Option<String>) -> Result<(), String> {
        Ok(())
    }
}

/// Exports the lapp backend `$lapp`, a type implementing [`Lapp`].
#[macro_export]
macro_rules! export {
    ($lapp:ty) => {
        #[doc(hidden)]
        struct __LaplaceBack;

        impl $crate::bindings::exports::laplace::lapp::handler::Guest for __LaplaceBack {
            fn handle(
                request: $crate::bindings::exports::laplace::lapp::types::Request,
            ) -> $crate::bindings::exports::laplace::lapp::types::Response {
                $crate::convert::into_wit_response(<$lapp as $crate::Lapp>::handle($crate::convert::from_wit_request(
                    request,
                )))
            }
        }

        impl $crate::bindings::exports::laplace::lapp::lifecycle::Guest for __LaplaceBack {
            fn init(previous_version: Option<String>) -> Result<(), String> {
                <$lapp as $crate::Lapp>::init(previous_version)
            }
        }

        $crate::bindings::export_bindings!(__LaplaceBack with_types_in $crate::bindings);
    };
}
