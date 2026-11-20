//! Networks of the outgoing HTTP of a lapp backend: a CIDR, the local network of the user (`lan`) or
//! the device itself (`loopback`).

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An address range: `192.168.1.0/24`, `fd00::/8`; an address without a prefix is the address alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpNet {
    addr: IpAddr,
    prefix: u8,
}

impl IpNet {
    const fn v4(a: u8, b: u8, c: u8, d: u8, prefix: u8) -> Self {
        Self {
            addr: IpAddr::V4(Ipv4Addr::new(a, b, c, d)),
            prefix,
        }
    }

    const fn v6(first: u16, prefix: u8) -> Self {
        Self {
            addr: IpAddr::V6(Ipv6Addr::new(first, 0, 0, 0, 0, 0, 0, 0)),
            prefix,
        }
    }

    pub const fn prefix(&self) -> u8 {
        self.prefix
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => same_prefix(&net.octets(), &ip.octets(), self.prefix),
            (IpAddr::V6(net), IpAddr::V6(ip)) => same_prefix(&net.octets(), &ip.octets(), self.prefix),
            // An IPv4-mapped IPv6 address is the IPv4 one.
            (IpAddr::V4(_), IpAddr::V6(ip)) => ip.to_ipv4_mapped().is_some_and(|ip| self.contains(IpAddr::V4(ip))),
            (IpAddr::V6(_), IpAddr::V4(_)) => false,
        }
    }

    /// Whether `other` lies wholly inside this one.
    pub fn contains_net(&self, other: &Self) -> bool {
        other.prefix >= self.prefix && self.contains(other.addr)
    }
}

fn same_prefix(net: &[u8], ip: &[u8], prefix: u8) -> bool {
    let (bytes, bits) = (usize::from(prefix / 8), prefix % 8);
    net[..bytes] == ip[..bytes] && (bits == 0 || (net[bytes] ^ ip[bytes]) >> (8 - bits) == 0)
}

impl FromStr for IpNet {
    type Err = String;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        let (addr, prefix) = source.split_once('/').unwrap_or((source, ""));
        let addr: IpAddr = addr
            .parse()
            .map_err(|_| format!("'{source}' is not a network address"))?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = if prefix.is_empty() {
            max
        } else {
            prefix
                .parse::<u8>()
                .ok()
                .filter(|&prefix| prefix <= max)
                .ok_or_else(|| format!("'{source}' has an invalid prefix length"))?
        };
        Ok(Self { addr, prefix })
    }
}

impl fmt::Display for IpNet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

/// The local network of the user: private, shared (CGNAT and overlay networks such as Tailscale),
/// link-local and unique local addresses; loopback is not in it.
const LAN: [IpNet; 7] = [
    IpNet::v4(10, 0, 0, 0, 8),
    IpNet::v4(172, 16, 0, 0, 12),
    IpNet::v4(192, 168, 0, 0, 16),
    IpNet::v4(100, 64, 0, 0, 10),
    IpNet::v4(169, 254, 0, 0, 16),
    IpNet::v6(0xfc00, 7),
    IpNet::v6(0xfe80, 10),
];

/// The device itself.
const LOOPBACK: [IpNet; 2] = [IpNet::v4(127, 0, 0, 0, 8), IpNet {
    addr: IpAddr::V6(Ipv6Addr::LOCALHOST),
    prefix: 128,
}];

/// The network of an entry of the outgoing HTTP of a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Lan,
    Loopback,
    Net(IpNet),
}

impl Network {
    fn nets(&self) -> &[IpNet] {
        match self {
            Self::Lan => &LAN,
            Self::Loopback => &LOOPBACK,
            Self::Net(net) => std::slice::from_ref(net),
        }
    }

    /// The prefix length of the range of the network that `ip` is in: the longer, the closer the
    /// entry.
    pub fn matches(&self, ip: IpAddr) -> Option<u8> {
        self.nets()
            .iter()
            .filter(|net| net.contains(ip))
            .map(IpNet::prefix)
            .max()
    }

    /// Whether `other` lies wholly inside this network.
    pub fn contains(&self, other: &Self) -> bool {
        other
            .nets()
            .iter()
            .all(|inner| self.nets().iter().any(|outer| outer.contains_net(inner)))
    }
}

impl FromStr for Network {
    type Err = String;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        match source {
            "lan" => Ok(Self::Lan),
            "loopback" => Ok(Self::Loopback),
            net => net.parse().map(Self::Net),
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lan => f.write_str("lan"),
            Self::Loopback => f.write_str("loopback"),
            Self::Net(net) => net.fmt(f),
        }
    }
}

impl Serialize for Network {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Network {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(ip: &str) -> IpAddr {
        ip.parse().unwrap()
    }

    fn network(network: &str) -> Network {
        network.parse().unwrap()
    }

    #[test]
    fn networks_parse() {
        assert_eq!(network("lan"), Network::Lan);
        assert_eq!(network("loopback"), Network::Loopback);
        assert_eq!(network("192.168.1.0/24").to_string(), "192.168.1.0/24");
        assert_eq!(network("fd00::/8").to_string(), "fd00::/8");
        assert_eq!(network("10.0.0.7").to_string(), "10.0.0.7/32");
        for invalid in ["192.168.1.0/33", "fd00::/129", "example.org", "10.0.0.0/x", ""] {
            assert!(invalid.parse::<Network>().is_err(), "{invalid}");
        }
    }

    #[test]
    fn addresses_in_networks() {
        let net = network("192.168.1.0/24");
        assert_eq!(net.matches(ip("192.168.1.20")), Some(24));
        assert_eq!(net.matches(ip("192.168.2.1")), None);
        assert_eq!(net.matches(ip("::ffff:192.168.1.20")), Some(24));
        assert_eq!(network("10.1.0.0/17").matches(ip("10.1.127.255")), Some(17));
        assert_eq!(network("10.1.0.0/17").matches(ip("10.1.128.0")), None);

        for local in [
            "10.0.0.1",
            "172.31.0.1",
            "192.168.0.1",
            "100.100.1.1",
            "169.254.1.1",
            "fd00::1",
            "fe80::1",
        ] {
            assert!(Network::Lan.matches(ip(local)).is_some(), "{local}");
        }
        for not_lan in ["127.0.0.1", "::1", "8.8.8.8", "172.32.0.1", "2001:db8::1"] {
            assert!(Network::Lan.matches(ip(not_lan)).is_none(), "{not_lan}");
        }
        assert!(Network::Loopback.matches(ip("127.1.2.3")).is_some());
        assert!(Network::Loopback.matches(ip("::1")).is_some());
    }

    #[test]
    fn networks_inside_networks() {
        assert!(Network::Lan.contains(&network("192.168.1.0/24")));
        assert!(Network::Lan.contains(&network("fd12::/16")));
        assert!(!Network::Lan.contains(&network("127.0.0.0/8")));
        assert!(!Network::Lan.contains(&Network::Loopback));
        assert!(!network("192.168.1.0/24").contains(&Network::Lan));
        assert!(network("10.0.0.0/8").contains(&network("10.2.0.0/16")));
        assert!(!network("10.2.0.0/16").contains(&network("10.0.0.0/8")));
        assert!(Network::Loopback.contains(&network("127.0.0.1")));
    }
}
