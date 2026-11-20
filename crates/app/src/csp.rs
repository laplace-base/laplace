//! The `Content-Security-Policy` of the lapp windows: the only way to keep the external requests of
//! a frontend in check that every engine has — WebKitGTK and WKWebView cannot intercept them.

use laplace_settings::lapp::access::FrontPermission;
use laplace_settings::lapp::{EffectiveHosts, PermissionsSettings};

use crate::assets::lapp_scheme;

/// The policy of the window of the lapp `lapp_name`, given its effective permissions and the hosts
/// its frontend effectively reaches over HTTP.
///
/// Without `http` and `websocket` the frontend reaches nothing but its own origin, in both forms,
/// and the Tauri IPC; with them, only `hosts`.
pub fn policy(lapp_name: &str, permissions: &PermissionsSettings, hosts: &EffectiveHosts) -> String {
    let scheme = lapp_scheme(lapp_name);
    let own = format!("'self' {scheme}: http://{scheme}.localhost https://{scheme}.localhost");

    let sources = |permission, scheme: &str| {
        let mut sources = String::new();

        if !permissions.front.contains(&permission) {
            return sources;
        }

        match hosts {
            EffectiveHosts::All(_) => sources.push_str(&format!(" {scheme}:")),
            EffectiveHosts::List(hosts) => {
                for host in hosts.iter().filter_map(|entry| entry.host()) {
                    if is_valid_host(host) {
                        sources.push_str(&format!(" {scheme}://{host}"));
                    }
                }
            },
        }

        sources
    };

    let http = sources(FrontPermission::Http, "https");
    let websocket = sources(FrontPermission::Websocket, "wss");

    [
        format!("default-src {own}; script-src {own} 'wasm-unsafe-eval'"),
        // The components set inline `style` attributes and htmx adds its indicator styles.
        format!("style-src {own} 'unsafe-inline'"),
        format!("connect-src {own} ipc: http://ipc.localhost https://ipc.localhost{http}{websocket}"),
        format!("img-src {own} data: blob:{http}"),
        "frame-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'".into(),
    ]
    .join("; ")
}

/// Whether `host` from a manifest can go into a policy as it is: a host name, a `*.` wildcard of one
/// or a port — nothing that would end the source list or start another directive.
fn is_valid_host(host: &str) -> bool {
    let valid = !host.is_empty()
        && host
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'-' | b'*' | b':'));
    if !valid {
        log::warn!("Host '{host}' of a lapp manifest is not a host name and is left out of the policy");
    }
    valid
}

#[cfg(test)]
mod tests {
    use laplace_settings::lapp::{EffectiveHost, HostTarget, HttpHosts, HttpMethods};

    use super::*;

    fn effective_hosts(hosts: &[&str]) -> EffectiveHosts {
        EffectiveHosts::List(
            hosts
                .iter()
                .map(|&host| EffectiveHost {
                    target: HostTarget::Host(host.into()),
                    methods: HttpMethods::All,
                })
                .collect(),
        )
    }

    fn permissions(front: &[FrontPermission]) -> PermissionsSettings {
        PermissionsSettings {
            front: front.to_vec(),
            back: Vec::new(),
        }
    }

    fn directive<'a>(policy: &'a str, name: &str) -> &'a str {
        policy
            .split("; ")
            .find(|directive| directive.starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("no {name} in {policy}"))
    }

    #[test]
    fn without_permissions_only_the_own_origin_and_ipc() {
        let policy = policy(
            "echo",
            &permissions(&[FrontPermission::InvokeBack]),
            &EffectiveHosts::All(HttpMethods::All),
        );

        assert_eq!(
            directive(&policy, "connect-src"),
            "connect-src 'self' lapp-echo: http://lapp-echo.localhost https://lapp-echo.localhost ipc: \
             http://ipc.localhost https://ipc.localhost"
        );
        assert!(directive(&policy, "script-src").ends_with("'wasm-unsafe-eval'"));
        assert_eq!(
            directive(&policy, "img-src"),
            "img-src 'self' lapp-echo: http://lapp-echo.localhost https://lapp-echo.localhost data: blob:"
        );
        assert!(!policy.contains("wss:"));
        for closed in ["frame-src", "object-src", "form-action", "base-uri"] {
            assert_eq!(directive(&policy, closed), format!("{closed} 'none'"));
        }
    }

    #[test]
    fn http_opens_the_manifest_hosts() {
        let hosts = effective_hosts(&["api.example.org", "*.cdn.example.org"]);
        let policy = policy("echo", &permissions(&[FrontPermission::Http]), &hosts);

        for name in ["connect-src", "img-src"] {
            let directive = directive(&policy, name);
            assert!(directive.contains(" https://api.example.org"), "{directive}");
            assert!(directive.contains(" https://*.cdn.example.org"), "{directive}");
        }
        assert!(!policy.contains("wss:"));
    }

    #[test]
    fn http_without_hosts_opens_nothing() {
        let policy = policy(
            "echo",
            &permissions(&[FrontPermission::Http]),
            &EffectiveHosts::default(),
        );

        for name in ["connect-src", "img-src"] {
            let directive = directive(&policy, name);
            assert!(
                directive
                    .split(' ')
                    .all(|source| source != "https:" && !source.contains("example")),
                "{directive}"
            );
        }
    }

    #[test]
    fn http_with_all_hosts_opens_https() {
        let policy = policy(
            "echo",
            &permissions(&[FrontPermission::Http]),
            &EffectiveHosts::All(HttpMethods::All),
        );

        assert!(directive(&policy, "connect-src").ends_with(" https:"));
        assert!(directive(&policy, "img-src").ends_with(" https:"));
    }

    #[test]
    fn websocket_opens_wss_to_the_manifest_hosts() {
        let hosts = effective_hosts(&["ws.example.org"]);
        let policy = policy("echo", &permissions(&[FrontPermission::Websocket]), &hosts);

        let connect = directive(&policy, "connect-src");
        assert!(connect.ends_with(" wss://ws.example.org"), "{connect}");
        assert!(!connect.contains("https://ws.example.org"));
        assert!(!directive(&policy, "img-src").contains("example"));
    }

    #[test]
    fn a_host_that_is_not_a_host_name_is_left_out() {
        let hosts = effective_hosts(&["evil.org; script-src *", "ok.org"]);
        let policy = policy("echo", &permissions(&[FrontPermission::Http]), &hosts);

        assert!(!policy.contains("evil.org"));
        assert!(!policy.contains("script-src *"));
        assert!(directive(&policy, "connect-src").contains(" https://ok.org"));
    }

    #[test]
    fn user_host_is_in_the_policy_with_user_hosts_only() {
        let requested = HttpHosts::List(vec!["api.example.org".into()]);
        let allowed = HttpHosts::List(vec!["api.example.org".into(), "own.example.net".into()]);
        let policy_with = |user_hosts| {
            let hosts = EffectiveHosts::resolve(&requested, user_hosts, &HttpMethods::All, &allowed);
            policy("echo", &permissions(&[FrontPermission::Http]), &hosts)
        };

        let with = policy_with(true);
        assert!(directive(&with, "connect-src").contains("https://api.example.org"));
        assert!(directive(&with, "connect-src").contains("https://own.example.net"));

        let without = policy_with(false);
        assert!(directive(&without, "connect-src").contains("https://api.example.org"));
        assert!(!without.contains("own.example.net"));
    }
}
