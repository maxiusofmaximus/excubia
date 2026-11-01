//! The local core's LAN side: the private address it listens on for phones,
//! and the pairing link a phone reads from the QR shown here.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use herdr_desktop_core::PairLink;
use qrcode::render::svg;
use qrcode::QrCode;

const DEFAULT_LAN_PORT: u16 = 8443;
const LAN_PORT_VAR: &str = "EXCUBIA_LAN_PORT";

/// The LAN port: `EXCUBIA_LAN_PORT` when it is 1024-65535, else the default.
pub fn lan_port() -> u16 {
    lan_port_from(std::env::var(LAN_PORT_VAR).ok().as_deref())
}

fn lan_port_from(value: Option<&str>) -> u16 {
    match value {
        None => DEFAULT_LAN_PORT,
        Some(value) => match value.trim().parse::<u16>() {
            Ok(port) if port >= 1024 => port,
            _ => {
                eprintln!("{LAN_PORT_VAR}={value} no es un puerto entre 1024 y 65535; uso {DEFAULT_LAN_PORT}");
                DEFAULT_LAN_PORT
            }
        },
    }
}

/// The private IPv4 this machine sends from. A connected UDP socket sends no
/// packet; it only asks the OS which interface routes out.
pub fn lan_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(address) if address.is_private() => Some(address),
        _ => None,
    }
}

/// The concrete address the core's LAN listener binds, never a wildcard.
pub fn lan_bind() -> Option<SocketAddr> {
    lan_address().map(|address| SocketAddr::new(address.into(), lan_port()))
}

/// Pulls the link out of `host pair-code --lan`'s output.
pub fn pair_link_from(stdout: &str) -> Result<String, String> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("pairing link: "))
        .map(|link| link.trim().to_string())
        .filter(|link| PairLink::parse_now(link).is_ok())
        .ok_or_else(|| "el host no devolvió un enlace de emparejamiento".to_string())
}

/// The link as an SVG QR code, drawn here so no service sees the link.
pub fn qr_svg(link: &str) -> Result<String, String> {
    let code = QrCode::new(link.as_bytes()).map_err(|error| error.to_string())?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_line_is_found_in_the_host_output() {
        let stdout = "pairing code: ABCD-2345\nsingle use\npairing link: herdr-notify://pair?v=1&c=ABCD-2345&h=-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI&a=192.168.1.20:8443&e=4102444800\ndata dir: x\n";
        let link = pair_link_from(stdout).expect("link");
        assert!(link.starts_with("herdr-notify://pair?v=1&c=ABCD-2345"));
    }

    #[test]
    fn a_host_without_a_link_is_an_error() {
        assert!(pair_link_from("pairing code: ABCD-2345\n").is_err());
    }

    #[test]
    fn the_lan_port_defaults_to_8443_and_takes_a_valid_override() {
        assert_eq!(lan_port_from(None), 8443);
        assert_eq!(lan_port_from(Some("9443")), 9443);
        assert_eq!(lan_port_from(Some(" 65535 ")), 65535);
    }

    #[test]
    fn an_invalid_or_out_of_range_lan_port_falls_back_to_8443() {
        assert_eq!(lan_port_from(Some("abc")), 8443);
        assert_eq!(lan_port_from(Some("")), 8443);
        assert_eq!(lan_port_from(Some("1023")), 8443);
        assert_eq!(lan_port_from(Some("70000")), 8443);
    }

    #[test]
    fn the_qr_is_an_svg_document() {
        let svg = qr_svg("herdr-notify://pair?v=1").expect("qr");
        assert!(
            svg.starts_with("<?xml") || svg.contains("<svg"),
            "{svg:.80}"
        );
    }
}
