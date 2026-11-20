//! The outgoing HTTP of a lapp backend: `wasi:http`, let through only with the `http` permission of
//! the back, to its effective hosts, by the methods and within the timeout of its manifest.

use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use http::uri::{Authority, Scheme};
use http_body_util::BodyExt;
use laplace_settings::lapp::{HttpHosts, HttpMethods};
use wasmtime_wasi_http::{Error, RequestOptions, WasiBody, WasiHttpHooks, default_send_request};

/// What the backend of a lapp may reach over HTTP.
#[derive(Debug, Clone)]
pub struct HttpAccess {
    pub lapp_name: String,
    /// Whether the back has the effective `http` permission.
    pub permitted: bool,
    /// The effective hosts of the back: `host` or `host:port`, `*.domain`, or all.
    pub hosts: HttpHosts,
    pub methods: HttpMethods,
    /// The limit of connecting, of the first byte and of every pause between bytes.
    pub timeout: Duration,
}

/// How the host of a request is among the hosts of the lapp; the greater is the closer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HostMatch {
    /// Through `*.` or `all`.
    Broad,
    /// Listed as it is.
    Exact,
}

impl HttpAccess {
    /// Checks `method` and `uri` of a request; `Ok(true)` if the host is a name reached through `*.`
    /// or `all`, whose addresses must not be local either.
    pub fn check(&self, method: &str, uri: &http::Uri) -> Result<bool, String> {
        if !self.permitted {
            return Err("no `http` permission".into());
        }

        if !self.methods.allows(method) {
            return Err(format!("the method {method} is not in the manifest"));
        }

        let scheme = uri.scheme().ok_or("no scheme")?;
        let default_port = if *scheme == Scheme::HTTPS {
            443
        } else if *scheme == Scheme::HTTP {
            80
        } else {
            return Err(format!("the scheme {scheme} is not HTTP"));
        };
        let host = uri.host().ok_or("no host")?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let port = uri.port_u16().unwrap_or(default_port);

        let matched = self
            .host_match(host, port, default_port)
            .ok_or_else(|| format!("the host {host}:{port} is not allowed"))?;
        let ip = host.parse::<IpAddr>().ok();

        if matched == HostMatch::Broad {
            if *scheme == Scheme::HTTP {
                return Err(format!("plain HTTP to {host} needs the host listed as it is"));
            }
            if is_local_name(host) || ip.is_some_and(is_local_ip) {
                return Err(format!("the local host {host} needs to be listed as it is"));
            }
        }

        Ok(matched == HostMatch::Broad && ip.is_none())
    }

    fn host_match(&self, host: &str, port: u16, default_port: u16) -> Option<HostMatch> {
        let entries = match &self.hosts {
            HttpHosts::All => return Some(HostMatch::Broad),
            HttpHosts::List(entries) => entries,
        };

        // The closest entry decides, whatever their order: a host listed as it is goes without the
        // checks of `*.`, even if a mask before it matches it too.
        entries
            .iter()
            .filter_map(|entry| {
                let entry = entry.parse::<Authority>().ok()?;
                if entry.port_u16().unwrap_or(default_port) != port {
                    return None;
                }
                let entry_host = entry.host().trim_start_matches('[').trim_end_matches(']');

                if entry_host.eq_ignore_ascii_case(host) {
                    Some(HostMatch::Exact)
                } else {
                    let domain = entry_host.strip_prefix("*.")?;
                    let subdomain = host.len().checked_sub(domain.len() + 1)?;
                    (host.is_char_boundary(subdomain)
                        && host[subdomain..].starts_with('.')
                        && host[subdomain + 1..].eq_ignore_ascii_case(domain))
                    .then_some(HostMatch::Broad)
                }
            })
            .max()
    }

    /// The timeouts of the guest, none longer than that of the manifest.
    fn options(&self, options: Option<RequestOptions>) -> RequestOptions {
        let limit = |timeout: Option<Duration>| Some(timeout.map_or(self.timeout, |timeout| timeout.min(self.timeout)));
        let options = options.unwrap_or_default();

        RequestOptions {
            connect_timeout: limit(options.connect_timeout),
            first_byte_timeout: limit(options.first_byte_timeout),
            between_bytes_timeout: limit(options.between_bytes_timeout),
        }
    }

    /// Whether `host:port` resolves to a local address. The request then resolves the name anew, so a
    /// name that changes its addresses in between (DNS rebinding) is not caught.
    async fn resolves_locally(&self, host: &str, port: u16) -> bool {
        match tokio::net::lookup_host((host, port)).await {
            Ok(mut addresses) => addresses.any(|address| is_local_ip(address.ip())),
            // The request fails to resolve it the same way.
            Err(_) => false,
        }
    }
}

fn is_local_name(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost" || host.ends_with(".localhost")
}

/// Loopback, private, link-local, shared, unspecified and broadcast addresses.
fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64)
        },
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
                || ip.to_ipv4_mapped().is_some_and(|ip| is_local_ip(IpAddr::V4(ip)))
        },
    }
}

/// The `wasi:http` hooks of an instance: every outgoing request goes through [`HttpAccess`].
pub struct HttpHooks(Arc<HttpAccess>);

impl HttpHooks {
    pub fn new(access: HttpAccess) -> Self {
        Self(Arc::new(access))
    }
}

type ResponseFuture = Box<
    dyn Future<
            Output = Result<
                (
                    http::Response<WasiBody>,
                    Box<dyn Future<Output = Result<(), Error>> + Send>,
                ),
                Error,
            >,
        > + Send,
>;

impl WasiHttpHooks for HttpHooks {
    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        options: Option<RequestOptions>,
        _fut: Box<dyn Future<Output = Result<(), Error>> + Send>,
    ) -> ResponseFuture {
        let access = self.0.clone();

        Box::new(async move {
            let method = request.method();
            let uri = request.uri();
            let deny = |reason: String| {
                log::warn!(
                    "Lapp '{}' is denied the request {method} {uri}: {reason}",
                    access.lapp_name
                );
                Error::HttpRequestDenied
            };

            let resolve = access.check(method.as_str(), uri).map_err(deny)?;
            if resolve {
                let host = uri.host().unwrap_or_default();
                let port = uri.port_u16().unwrap_or(443);
                if access.resolves_locally(host, port).await {
                    return Err(deny(format!("{host} resolves to a local address")));
                }
            }

            let (response, io) = default_send_request(request, Some(access.options(options))).await?;
            Ok((
                response.map(BodyExt::boxed_unsync),
                Box::new(io) as Box<dyn Future<Output = Result<(), Error>> + Send>,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(hosts: HttpHosts) -> HttpAccess {
        HttpAccess {
            lapp_name: "test".into(),
            permitted: true,
            hosts,
            methods: HttpMethods::All,
            timeout: Duration::from_secs(1),
        }
    }

    fn list(hosts: &[&str]) -> HttpHosts {
        HttpHosts::List(hosts.iter().map(|host| host.to_string()).collect())
    }

    fn check(access: &HttpAccess, method: &str, uri: &str) -> Result<bool, String> {
        access.check(method, &uri.parse().unwrap())
    }

    #[test]
    fn listed_hosts_only() {
        let access = access(list(&["api.example.org", "*.cdn.example.org", "example.com:8443"]));

        assert_eq!(check(&access, "GET", "https://api.example.org/x"), Ok(false));
        assert_eq!(check(&access, "GET", "https://API.example.org/x"), Ok(false));
        assert_eq!(check(&access, "GET", "https://a.cdn.example.org/x"), Ok(true));
        assert_eq!(check(&access, "GET", "https://example.com:8443/x"), Ok(false));

        for denied in [
            "https://cdn.example.org/x",
            "https://evilcdn.example.org/x",
            "https://example.org/x",
            "https://api.example.org:8443/x",
            "https://example.com/x",
        ] {
            assert!(check(&access, "GET", denied).is_err(), "{denied}");
        }
    }

    #[test]
    fn listed_host_is_exact_whatever_the_order_of_the_entries() {
        for hosts in [["*.example.org", "api.example.org"], [
            "api.example.org",
            "*.example.org",
        ]] {
            let access = access(list(&hosts));
            assert_eq!(
                check(&access, "GET", "https://api.example.org/"),
                Ok(false),
                "{hosts:?}"
            );
            assert_eq!(check(&access, "GET", "https://cdn.example.org/"), Ok(true), "{hosts:?}");
        }

        let access = access(list(&["*.local", "nas.local:8080"]));
        assert_eq!(check(&access, "GET", "http://nas.local:8080/"), Ok(false));
        assert!(check(&access, "GET", "http://printer.local/").is_err());
    }

    #[test]
    fn no_permission_no_requests() {
        let access = HttpAccess {
            permitted: false,
            ..access(HttpHosts::All)
        };
        assert!(check(&access, "GET", "https://example.org/").is_err());
    }

    #[test]
    fn methods_of_the_manifest_only() {
        let access = HttpAccess {
            methods: HttpMethods::List(vec![laplace_settings::lapp::HttpMethod::Get]),
            ..access(HttpHosts::All)
        };
        assert!(check(&access, "GET", "https://example.org/").is_ok());
        assert!(check(&access, "POST", "https://example.org/").is_err());
    }

    #[test]
    fn plain_http_and_local_hosts_need_an_exact_entry() {
        let all = access(HttpHosts::All);
        assert_eq!(check(&all, "GET", "https://example.org/"), Ok(true));
        for denied in [
            "http://example.org/",
            "https://127.0.0.1/",
            "https://localhost/",
            "https://app.localhost/",
            "https://192.168.1.1/",
            "https://10.0.0.1/",
            "https://169.254.169.254/",
            "https://[::1]/",
            "https://[fd00::1]/",
            "https://[::ffff:127.0.0.1]/",
        ] {
            assert!(check(&all, "GET", denied).is_err(), "{denied}");
        }

        let listed = access(list(&["127.0.0.1:8080", "[::1]:8080", "localhost:8080"]));
        assert_eq!(check(&listed, "GET", "http://127.0.0.1:8080/"), Ok(false));
        assert_eq!(check(&listed, "GET", "http://[::1]:8080/"), Ok(false));
        assert_eq!(check(&listed, "GET", "http://localhost:8080/"), Ok(false));
        assert!(check(&listed, "GET", "http://127.0.0.1:8081/").is_err());
    }

    #[test]
    fn timeouts_are_limited_by_the_manifest() {
        let access = access(HttpHosts::All);
        let options = access.options(Some(RequestOptions {
            connect_timeout: Some(Duration::from_millis(100)),
            first_byte_timeout: Some(Duration::from_secs(60)),
            between_bytes_timeout: None,
        }));

        assert_eq!(options.connect_timeout, Some(Duration::from_millis(100)));
        assert_eq!(options.first_byte_timeout, Some(Duration::from_secs(1)));
        assert_eq!(options.between_bytes_timeout, Some(Duration::from_secs(1)));
    }
}
