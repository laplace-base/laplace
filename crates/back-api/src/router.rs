//! Routing of the requests of a lapp frontend by method and path.

use http::{Method, Request, Response, StatusCode};

/// A request handler.
pub type Handler = fn(Request<Vec<u8>>) -> Response<Vec<u8>>;

/// The path of a request to the backend of a lapp, `/<name>/api/<path>`, without its prefix:
/// `/<path>`. `None` for a path outside of `/<name>/api`.
pub fn api_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix('/')?;
    let (_name, rest) = rest.split_once('/')?;
    let rest = rest.strip_prefix("api")?;
    match rest {
        "" => Some("/"),
        rest if rest.starts_with('/') => Some(rest),
        _ => None,
    }
}

/// Routes by method and exact path relative to `/<name>/api` ([`api_path`]).
///
/// No route for the path is `404`, a route for it but not for the method is `405`, unless there is
/// a fallback, which gets both.
#[derive(Default)]
pub struct Router {
    routes: Vec<(Method, &'static str, Handler)>,
    fallback: Option<Handler>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route(mut self, method: Method, path: &'static str, handler: Handler) -> Self {
        self.routes.push((method, path, handler));
        self
    }

    pub fn get(self, path: &'static str, handler: Handler) -> Self {
        self.route(Method::GET, path, handler)
    }

    pub fn post(self, path: &'static str, handler: Handler) -> Self {
        self.route(Method::POST, path, handler)
    }

    /// The handler of the requests no route matches.
    pub fn fallback(mut self, handler: Handler) -> Self {
        self.fallback = Some(handler);
        self
    }

    pub fn handle(&self, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        let path = api_path(request.uri().path());
        let mut path_matches = false;

        if let Some(path) = path {
            for (method, route, handler) in &self.routes {
                if *route == path {
                    if method == request.method() {
                        return handler(request);
                    }
                    path_matches = true;
                }
            }
        }

        match self.fallback {
            Some(fallback) => fallback(request),
            None if path_matches => status(StatusCode::METHOD_NOT_ALLOWED),
            None => status(StatusCode::NOT_FOUND),
        }
    }
}

fn status(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(status.canonical_reason().unwrap_or_default().as_bytes().to_vec());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: Method, path: &str) -> Request<Vec<u8>> {
        Request::builder().method(method).uri(path).body(Vec::new()).unwrap()
    }

    fn named(name: &'static str) -> Response<Vec<u8>> {
        Response::new(name.as_bytes().to_vec())
    }

    fn hello(_: Request<Vec<u8>>) -> Response<Vec<u8>> {
        named("hello")
    }

    fn create(_: Request<Vec<u8>>) -> Response<Vec<u8>> {
        named("create")
    }

    fn rest(_: Request<Vec<u8>>) -> Response<Vec<u8>> {
        named("rest")
    }

    #[test]
    fn api_prefix_is_stripped() {
        assert_eq!(api_path("/echo/api/hello"), Some("/hello"));
        assert_eq!(api_path("/echo/api/a/b"), Some("/a/b"));
        assert_eq!(api_path("/echo/api"), Some("/"));
        assert_eq!(api_path("/echo/api/"), Some("/"));
        assert_eq!(api_path("/echo/apix"), None);
        assert_eq!(api_path("/echo/index.html"), None);
        assert_eq!(api_path("/echo"), None);
    }

    #[test]
    fn routes_by_method_and_path() {
        let router = Router::new().get("/hello", hello).post("/hello", create);

        assert_eq!(router.handle(request(Method::GET, "/echo/api/hello")).body(), b"hello");
        assert_eq!(
            router.handle(request(Method::POST, "/echo/api/hello?x=1")).body(),
            b"create"
        );
        assert_eq!(
            router.handle(request(Method::DELETE, "/echo/api/hello")).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            router.handle(request(Method::GET, "/echo/api/other")).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            router.handle(request(Method::GET, "/echo/hello")).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn fallback_gets_the_rest() {
        let router = Router::new().get("/hello", hello).fallback(rest);

        assert_eq!(router.handle(request(Method::GET, "/echo/api/hello")).body(), b"hello");
        assert_eq!(router.handle(request(Method::PUT, "/echo/api/hello")).body(), b"rest");
        assert_eq!(router.handle(request(Method::GET, "/elsewhere")).body(), b"rest");
    }
}
