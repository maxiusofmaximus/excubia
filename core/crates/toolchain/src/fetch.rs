//! Downloading the pinned archive. The URL comes from the manifest only; the
//! redirect policy accepts only HTTPS to GitHub's release hosts.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::redirect::Policy;

/// Writes the body of a URL to a file, never more than `max_bytes`.
pub trait Fetch: Send + Sync {
    fn fetch(&self, url: &str, max_bytes: u64, to: &Path) -> Result<(), String>;
}

/// The production fetcher: HTTPS only, GitHub release hosts only, a time limit.
pub struct HttpsFetch;

const ALLOWED_HOSTS: [&str; 3] = [
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

impl Fetch for HttpsFetch {
    fn fetch(&self, url: &str, max_bytes: u64, to: &Path) -> Result<(), String> {
        let client = Client::builder()
            .redirect(Policy::custom(|attempt| {
                let allowed = attempt.url().scheme() == "https"
                    && attempt
                        .url()
                        .host_str()
                        .is_some_and(|host| ALLOWED_HOSTS.contains(&host));
                if allowed && attempt.previous().len() < 5 {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|error| format!("client: {error}"))?;
        let mut response = client
            .get(url)
            .send()
            .map_err(|error| format!("request: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status().as_u16()));
        }
        let mut file = File::create(to).map_err(|error| format!("create: {error}"))?;
        let copied = io::copy(&mut (&mut response).take(max_bytes + 1), &mut file)
            .map_err(|error| format!("read: {error}"))?;
        if copied > max_bytes {
            return Err(format!("body larger than {max_bytes} bytes"));
        }
        Ok(())
    }
}
