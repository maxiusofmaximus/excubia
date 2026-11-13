//! A scripted HTTP transport for the mail tests: it answers in the order it was
//! given and remembers the URLs asked. The folder aliases are answered by path and
//! a `wellKnownName` select is refused, as Graph does. No socket is ever opened.
//!
//! Each test binary uses its own subset of these helpers.
#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use excubia_mail::{HttpResponse, HttpTransport, MailError, Redacted};
use serde_json::json;

pub const ACCESS: &str = "ACCESO-SECRETO-NO-DEBE-SALIR";
pub const REFRESH: &str = "REFRESCO-SECRETO-NO-DEBE-SALIR";
pub const DEVICE: &str = "CODIGO-DISPOSITIVO-PRIVADO";
pub const USER_CODE: &str = "ABCD-1234";

pub struct Scripted {
    answers: Mutex<VecDeque<HttpResponse>>,
    urls: Mutex<Vec<String>>,
    /// The URLs of the JSON POSTs asked, in order: the ones that may change the mailbox.
    posts: Mutex<Vec<String>>,
    /// The answers of `GET /me/mailFolders/{alias}?$select=id`; any other alias is a 404.
    aliases: HashMap<&'static str, HttpResponse>,
}

impl Scripted {
    pub fn new(answers: Vec<HttpResponse>) -> Self {
        Self {
            answers: Mutex::new(answers.into()),
            urls: Mutex::new(Vec::new()),
            posts: Mutex::new(Vec::new()),
            aliases: HashMap::new(),
        }
    }

    pub fn with_alias(mut self, alias: &'static str, answer: HttpResponse) -> Self {
        self.aliases.insert(alias, answer);
        self
    }

    pub fn asked(&self) -> Vec<String> {
        self.urls.lock().expect("urls").clone()
    }

    pub fn posted(&self) -> Vec<String> {
        self.posts.lock().expect("posts").clone()
    }

    fn next(&self, url: &str) -> Result<HttpResponse, MailError> {
        self.urls.lock().expect("urls").push(url.to_owned());
        if let Some(answer) = self.fixed_answer(url) {
            return Ok(answer);
        }
        self.answers
            .lock()
            .expect("answers")
            .pop_front()
            .ok_or_else(|| MailError::BadResponse(Redacted::new("no scripted answer left")))
    }

    /// The answers Graph gives without the script; `None` leaves the call to it.
    fn fixed_answer(&self, url: &str) -> Option<HttpResponse> {
        if url
            .split_once("$select=")
            .is_some_and(|(_, select)| select.contains("wellKnownName"))
        {
            return Some(HttpResponse {
                status: 400,
                body: json!({"error": {"code": "BadRequest"}}).to_string(),
            });
        }
        let (_, alias) = url
            .strip_suffix("?$select=id")?
            .rsplit_once("/me/mailFolders/")?;
        if alias.contains('/') {
            return None;
        }
        Some(
            self.aliases
                .get(alias)
                .cloned()
                .unwrap_or_else(|| HttpResponse {
                    status: 404,
                    body: json!({"error": {"code": "ErrorItemNotFound"}}).to_string(),
                }),
        )
    }
}

#[async_trait]
impl HttpTransport for Scripted {
    async fn post_form(
        &self,
        url: &str,
        _form: &[(&str, &str)],
    ) -> Result<HttpResponse, MailError> {
        self.next(url)
    }

    async fn get_json(&self, url: &str, _bearer: &str) -> Result<HttpResponse, MailError> {
        self.next(url)
    }

    async fn post_json(
        &self,
        url: &str,
        _bearer: &str,
        _body: &serde_json::Value,
    ) -> Result<HttpResponse, MailError> {
        self.posts.lock().expect("posts").push(url.to_owned());
        self.next(url)
    }

    async fn patch_json(
        &self,
        url: &str,
        _bearer: &str,
        _body: &serde_json::Value,
    ) -> Result<HttpResponse, MailError> {
        self.next(url)
    }

    async fn post_action(&self, url: &str, _bearer: &str) -> Result<HttpResponse, MailError> {
        self.next(url)
    }

    async fn put_bytes(
        &self,
        url: &str,
        _content_type: &str,
        _bytes: &[u8],
    ) -> Result<HttpResponse, MailError> {
        self.next(url)
    }

    async fn put_range(
        &self,
        url: &str,
        _content_range: &str,
        _bytes: Vec<u8>,
    ) -> Result<HttpResponse, MailError> {
        self.next(url)
    }

    async fn get_upload_status(&self, url: &str) -> Result<HttpResponse, MailError> {
        self.next(url)
    }
}

pub fn ok(body: serde_json::Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: body.to_string(),
    }
}

/// The answer of the device-code endpoint, with a zero interval so the poll
/// does not wait in real time.
pub fn device_answer() -> HttpResponse {
    ok(json!({
        "device_code": DEVICE,
        "user_code": USER_CODE,
        "verification_uri": "https://microsoft.com/devicelogin",
        "expires_in": 900,
        "interval": 0,
    }))
}

pub fn token_answer() -> HttpResponse {
    ok(json!({
        "access_token": ACCESS,
        "refresh_token": REFRESH,
        "expires_in": 3600,
    }))
}
