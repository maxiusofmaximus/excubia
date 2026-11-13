//! A transport that answers from a script and records every call, so the
//! tests exercise the real auth, Graph and upload code with zero network and
//! zero accounts.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use excubia_mail::{HttpResponse, HttpTransport, MailError, MAX_UPLOAD_CHUNK_BYTES};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    None,
    Form(Vec<(String, String)>),
    Json(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Call {
    pub method: &'static str,
    pub url: String,
    pub bearer: Option<String>,
    /// The `Content-Range` of an upload chunk.
    pub range: Option<String>,
    pub body: Body,
}

impl Call {
    fn new(method: &'static str, url: &str, bearer: Option<&str>, body: Body) -> Self {
        Self {
            method,
            url: url.to_owned(),
            bearer: bearer.map(str::to_owned),
            range: None,
            body,
        }
    }

    pub fn form_value(&self, key: &str) -> Option<&str> {
        match &self.body {
            Body::Form(pairs) => pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str()),
            _ => None,
        }
    }

    pub fn json_body(&self) -> Value {
        match &self.body {
            Body::Json(text) => serde_json::from_str(text).expect("a JSON body"),
            _ => panic!("the call has no JSON body: {self:?}"),
        }
    }

    /// The number of bytes an upload chunk carried.
    pub fn payload_len(&self) -> usize {
        match &self.body {
            Body::Bytes(bytes) => bytes.len(),
            _ => 0,
        }
    }

    pub fn text(&self) -> String {
        match &self.body {
            Body::None => String::new(),
            Body::Form(pairs) => pairs
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("&"),
            Body::Json(text) => text.clone(),
            Body::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub body: String,
}

impl MockResponse {
    pub fn ok(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
        }
    }

    pub fn created(body: impl Into<String>) -> Self {
        Self {
            status: 201,
            body: body.into(),
        }
    }

    pub fn status(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }
}

/// Graph has no `wellKnownName` folder property, so a select that asks for one is refused.
const BAD_SELECT: &str = r#"{"error":{"code":"BadRequest","message":"Could not find a property named 'wellKnownName'"}}"#;
const NOT_FOUND: &str = r#"{"error":{"code":"ErrorItemNotFound","message":"The specified object was not found in the store."}}"#;

fn asks_well_known_name(url: &str) -> bool {
    url.split_once("$select=")
        .is_some_and(|(_, select)| select.contains("wellKnownName"))
}

type Hook = Box<dyn Fn(&Call) + Send + Sync>;

pub struct MockTransport {
    answers: Mutex<VecDeque<MockResponse>>,
    calls: Mutex<Vec<Call>>,
    /// Runs on every call before it is answered, e.g. to change a file mid-test.
    hook: Option<Hook>,
    /// The answers of `GET /me/mailFolders/{alias}?$select=id`. An alias not listed
    /// is a 404, as a mailbox without that folder answers.
    aliases: HashMap<&'static str, MockResponse>,
}

impl MockTransport {
    pub fn new(answers: Vec<MockResponse>) -> Self {
        Self {
            answers: Mutex::new(answers.into()),
            calls: Mutex::new(Vec::new()),
            hook: None,
            aliases: HashMap::new(),
        }
    }

    pub fn with_hook(mut self, hook: impl Fn(&Call) + Send + Sync + 'static) -> Self {
        self.hook = Some(Box::new(hook));
        self
    }

    pub fn with_alias(mut self, alias: &'static str, answer: MockResponse) -> Self {
        self.aliases.insert(alias, answer);
        self
    }

    /// The inbox and junk aliases, with the ids the folder fixtures give them.
    pub fn with_fixture_aliases(self) -> Self {
        self.with_alias("inbox", MockResponse::ok(r#"{"id":"F-INBOX"}"#))
            .with_alias("junkemail", MockResponse::ok(r#"{"id":"F-JUNK"}"#))
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn answer(&self, call: Call) -> MockResponse {
        if let Some(hook) = &self.hook {
            hook(&call);
        }
        let fixed = self.fixed_answer(&call.url);
        self.calls.lock().expect("calls lock").push(call);
        fixed.unwrap_or_else(|| {
            self.answers
                .lock()
                .expect("answers lock")
                .pop_front()
                .expect("the test script has an answer for every call")
        })
    }

    /// The answers Graph gives without the script: a refused `wellKnownName` select
    /// and the folder aliases by path. `None` leaves the call to the script.
    fn fixed_answer(&self, url: &str) -> Option<MockResponse> {
        if asks_well_known_name(url) {
            return Some(MockResponse::status(400, BAD_SELECT));
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
                .unwrap_or_else(|| MockResponse::status(404, NOT_FOUND)),
        )
    }

    fn reply(answer: MockResponse) -> Result<HttpResponse, MailError> {
        Ok(HttpResponse {
            status: answer.status,
            body: answer.body,
        })
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<HttpResponse, MailError> {
        let pairs = form
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        let answer = self.answer(Call::new("POST", url, None, Body::Form(pairs)));
        Self::reply(answer)
    }

    async fn get_json(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError> {
        let answer = self.answer(Call::new("GET", url, Some(bearer), Body::None));
        Self::reply(answer)
    }

    async fn post_json(
        &self,
        url: &str,
        bearer: &str,
        body: &Value,
    ) -> Result<HttpResponse, MailError> {
        let answer = self.answer(Call::new(
            "POST",
            url,
            Some(bearer),
            Body::Json(body.to_string()),
        ));
        Self::reply(answer)
    }

    async fn patch_json(
        &self,
        url: &str,
        bearer: &str,
        body: &Value,
    ) -> Result<HttpResponse, MailError> {
        let answer = self.answer(Call::new(
            "PATCH",
            url,
            Some(bearer),
            Body::Json(body.to_string()),
        ));
        Self::reply(answer)
    }

    async fn post_action(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError> {
        let answer = self.answer(Call::new("POST", url, Some(bearer), Body::None));
        Self::reply(answer)
    }

    async fn put_bytes(
        &self,
        url: &str,
        _content_type: &str,
        bytes: &[u8],
    ) -> Result<HttpResponse, MailError> {
        let answer = self.answer(Call::new("PUT", url, None, Body::Bytes(bytes.to_vec())));
        Self::reply(answer)
    }

    async fn put_range(
        &self,
        url: &str,
        content_range: &str,
        bytes: Vec<u8>,
    ) -> Result<HttpResponse, MailError> {
        assert!(
            bytes.len() <= MAX_UPLOAD_CHUNK_BYTES,
            "a chunk of {} bytes is over 4 MiB",
            bytes.len()
        );
        let mut call = Call::new("PUT", url, None, Body::Bytes(bytes));
        call.range = Some(content_range.to_owned());
        Self::reply(self.answer(call))
    }

    async fn get_upload_status(&self, url: &str) -> Result<HttpResponse, MailError> {
        let answer = self.answer(Call::new("GET", url, None, Body::None));
        Self::reply(answer)
    }
}
