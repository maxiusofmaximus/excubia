//! A link in a reply opens in the system browser, never inside the app. Only plain
//! http(s) addresses pass: a reply is untrusted text and must not start a program.

use std::path::Path;

use super::documents::open_with_system;

const MAX_URL: usize = 2048;

/// The address, when it is one the browser may open.
pub fn checked(url: &str) -> Option<&str> {
    let url = url.trim();
    let scheme_ok = url.starts_with("https://") || url.starts_with("http://");
    let clean = !url.chars().any(|c| {
        c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '`' | '^' | '|')
    });
    (scheme_ok && clean && url.len() <= MAX_URL && url.len() > "https://".len()).then_some(url)
}

#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    let url = checked(&url).ok_or_else(|| "ese enlace no se puede abrir".to_string())?;
    open_with_system(Path::new(url)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::checked;

    #[test]
    fn only_plain_web_addresses_open() {
        assert_eq!(
            checked("https://github.com/x"),
            Some("https://github.com/x")
        );
        assert_eq!(checked(" http://a.example "), Some("http://a.example"));
        assert_eq!(checked("file:///C:/Windows/notepad.exe"), None);
        assert_eq!(checked("javascript:alert(1)"), None);
        assert_eq!(checked("https://a.example/\" & calc"), None);
        assert_eq!(checked("https://"), None);
    }
}
