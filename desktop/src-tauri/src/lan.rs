//! The local core's LAN side: the private address it listens on for phones,
//! and the pairing link a phone reads from the QR shown here.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use herdr_desktop_core::PairLink;
use hn_core_pin::{unix_now, PakeLink};
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

/// Pulls the v2 URI (no address) out of `host pair-code --pake --uri`'s output.
/// Returns it with its expiry. Errors are fixed text: the URI holds a live code.
pub fn pake_uri_from(stdout: &str) -> Result<(String, i64), String> {
    let uri = stdout
        .lines()
        .find_map(|line| line.strip_prefix("pairing uri: "))
        .map(str::trim)
        .ok_or_else(|| "el host no devolvió un enlace de emparejamiento".to_string())?;
    let link = PakeLink::parse(uri)
        .map_err(|_| "el host devolvió un enlace de emparejamiento no válido".to_string())?;
    if link.is_expired(unix_now()) {
        return Err("el código de emparejamiento ya había caducado".to_string());
    }
    Ok((uri.to_string(), link.expires_at))
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

    const PIN: &str = "-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI";

    fn pake_output(expires: i64) -> String {
        format!(
            "código de emparejamiento: ABCD2345
válido 180 s (un solo uso)
pairing uri: herdr-notify://pair?v=2&c=ABCD2345&h={PIN}&e={expires}
"
        )
    }

    #[test]
    fn the_pake_uri_line_is_found_among_three_lines() {
        let (uri, expires) = pake_uri_from(&pake_output(4_102_444_800)).expect("uri");
        assert!(uri.starts_with("herdr-notify://pair?v=2&c=ABCD2345"));
        assert_eq!(expires, 4_102_444_800);
    }

    #[test]
    fn a_pake_output_without_the_uri_line_is_an_error() {
        assert!(pake_uri_from(
            "código de emparejamiento: ABCD2345
"
        )
        .is_err());
    }

    #[test]
    fn a_v1_uri_with_an_address_is_not_a_pake_uri() {
        let stdout = format!("pairing uri: herdr-notify://pair?v=1&c=ABCD2345&h={PIN}&a=192.168.1.20:8443&e=4102444800
");
        assert!(pake_uri_from(&stdout).is_err());
    }

    #[test]
    fn an_expired_pake_uri_is_an_error_and_the_error_hides_the_uri() {
        let error = pake_uri_from(&pake_output(1)).unwrap_err();
        assert!(
            !error.contains("ABCD2345") && !error.contains("herdr-notify"),
            "{error}"
        );
        let error = pake_uri_from(&pake_output(4_102_444_800).replace("v=2", "v=1")).unwrap_err();
        assert!(
            !error.contains("ABCD2345") && !error.contains(PIN),
            "{error}"
        );
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
