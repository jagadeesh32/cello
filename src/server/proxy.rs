//! Client address resolution behind reverse proxies.
//!
//! The TCP peer address is the only client identity the server can trust on its
//! own. `X-Forwarded-For` (and friends) are attacker-controlled unless the peer is
//! a proxy the operator configured. `App.set_trusted_proxies([...])` installs that
//! list for that app; with no list (the default) forwarded headers are ignored
//! entirely.

use std::collections::HashMap;
use std::net::IpAddr;

/// An IP network in CIDR form (`10.0.0.0/8`, `::1/128`, or a bare address).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// Parse `addr` or `addr/prefix`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (addr_part, prefix_part) = match s.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (s, None),
        };
        let addr: IpAddr = addr_part
            .parse()
            .map_err(|_| format!("invalid IP address in trusted proxy '{s}'"))?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix_part {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max)
                .ok_or_else(|| format!("invalid prefix length in trusted proxy '{s}'"))?,
            None => max,
        };
        Ok(Cidr { addr, prefix })
    }

    /// Whether `ip` falls inside this network.
    pub fn contains(&self, ip: &IpAddr) -> bool {
        // Compare IPv4-mapped IPv6 peers (`::ffff:10.0.0.1`) as IPv4.
        let ip = canonical(ip);
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX.checked_shl(32 - self.prefix as u32).unwrap_or(0);
                (u32::from(net) & mask) == (u32::from(ip) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX.checked_shl(128 - self.prefix as u32).unwrap_or(0);
                (u128::from(net) & mask) == (u128::from(ip) & mask)
            }
            _ => false,
        }
    }
}

fn canonical(ip: &IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(*v6)),
        v4 => *v4,
    }
}

/// The proxies one app trusts. Each `App` has its own list
/// (`App.set_trusted_proxies`); it reaches the server through `ServerConfig`.
#[derive(Clone, Debug, Default)]
pub struct TrustedProxies {
    list: Vec<Cidr>,
    /// Accept `X-Real-IP` from a trusted peer when there is no
    /// `X-Forwarded-For`. Off by default: a proxy that passes the inbound
    /// header through unchanged would let the caller pick its own address.
    trust_real_ip: bool,
}

impl TrustedProxies {
    pub fn new(list: Vec<Cidr>, trust_real_ip: bool) -> Self {
        TrustedProxies {
            list,
            trust_real_ip,
        }
    }

    /// Whether `ip` is a configured trusted proxy.
    pub fn is_trusted(&self, ip: &IpAddr) -> bool {
        !self.list.is_empty() && self.list.iter().any(|c| c.contains(ip))
    }

    /// Resolve the client address for a request received from `peer`.
    ///
    /// Returns `(client_ip, peer_is_trusted_proxy)`. When the peer is not
    /// trusted the client is the peer. When it is, `X-Forwarded-For` is walked
    /// right to left and the first address that is not itself a trusted proxy
    /// is the client (the left-most entries are whatever the client chose to
    /// send and are ignored). `X-Real-IP` is used only when enabled.
    pub fn resolve_client(
        &self,
        peer: IpAddr,
        headers: &HashMap<String, String>,
    ) -> (IpAddr, bool) {
        let peer = canonical(&peer);
        if !self.is_trusted(&peer) {
            return (peer, false);
        }
        if let Some(xff) = headers.get("x-forwarded-for") {
            let mut last_valid = None;
            for hop in xff.rsplit(',') {
                let Ok(ip) = hop.trim().parse::<IpAddr>() else {
                    // A malformed hop: stop here and use what we have.
                    break;
                };
                let ip = canonical(&ip);
                if !self.is_trusted(&ip) {
                    return (ip, true);
                }
                last_valid = Some(ip);
            }
            if let Some(ip) = last_valid {
                return (ip, true);
            }
        } else if self.trust_real_ip {
            if let Some(ip) = headers
                .get("x-real-ip")
                .and_then(|v| v.trim().parse::<IpAddr>().ok())
            {
                return (canonical(&ip), true);
            }
        }
        (peer, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_matching() {
        let c = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(c.contains(&"10.1.2.3".parse().unwrap()));
        assert!(!c.contains(&"11.0.0.1".parse().unwrap()));
        assert!(c.contains(&"::ffff:10.0.0.1".parse().unwrap()));
        let one = Cidr::parse("127.0.0.1").unwrap();
        assert!(one.contains(&"127.0.0.1".parse().unwrap()));
        assert!(!one.contains(&"127.0.0.2".parse().unwrap()));
        let all = Cidr::parse("0.0.0.0/0").unwrap();
        assert!(all.contains(&"8.8.8.8".parse().unwrap()));
        assert!(Cidr::parse("10.0.0.0/33").is_err());
        assert!(Cidr::parse("nope").is_err());
    }

    fn headers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn real_ip_needs_opt_in() {
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        let h = headers(&[("x-real-ip", "1.2.3.4")]);
        let off = TrustedProxies::new(vec![Cidr::parse("10.0.0.0/8").unwrap()], false);
        assert_eq!(off.resolve_client(peer, &h), (peer, true));
        let on = TrustedProxies::new(vec![Cidr::parse("10.0.0.0/8").unwrap()], true);
        assert_eq!(
            on.resolve_client(peer, &h).0,
            "1.2.3.4".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn untrusted_peer_ignores_headers() {
        let peer: IpAddr = "8.8.8.8".parse().unwrap();
        let h = headers(&[("x-forwarded-for", "1.2.3.4")]);
        let t = TrustedProxies::new(vec![Cidr::parse("10.0.0.0/8").unwrap()], true);
        assert_eq!(t.resolve_client(peer, &h), (peer, false));
        assert_eq!(
            TrustedProxies::default().resolve_client(peer, &h),
            (peer, false)
        );
    }
}
