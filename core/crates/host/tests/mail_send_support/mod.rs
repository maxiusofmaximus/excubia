//! The routed fake behind `mail_send`: one transport that answers Microsoft's
//! token and Graph endpoints from its own state, and a `Host` that wires a send
//! service to it. Zero network, zero real accounts.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hn_core_chat::{ChatStore, ContactFields, Decision, SeatActor};
use hn_core_host::mail_send::{MailSends, SendPolicy, SendStep};
use hn_core_host::mail_tools::MailTools;
use hn_core_mail::auth::{Tenant, Tokens};
use hn_core_mail::{
    Account, AccountStore, DraftBody, DraftInput, FileKey, GraphClient, HttpResponse,
    HttpTransport, Ledger, MailError, Redacted, Secret,
};
use hn_core_test_dir::TestDir;
use serde_json::{json, Value};

pub const ACCOUNT: &str = "trabajo";
pub const DRAFT: &str = "AAMk-draft-1";
pub const TO: &str = "ana@example.org";
pub const BODY: &str = "Hola Ana, adjunto el presupuesto de marzo.";
pub const LOGIN_ACCESS: &str = "login-access-token";
pub const LOGIN_REFRESH: &str = "login-refresh-secreto";
pub const SEND_ACCESS: &str = "send-access-token";
pub const SEND_REFRESH: &str = "send-refresh-secreto";
const OWNER: &str = "owner@example.com";
const SUBJECT: &str = "Presupuesto";

/// What the fake Microsoft endpoints hold: the draft's body, the bearer of every
/// read of the draft, and the bearer of every POST to `/send`.
struct World {
    subject: String,
    body: String,
    /// The `contentType` Graph stores with the body: what the host wrote, or `text`.
    content_type: String,
    /// How many drafts were created: the first is `DRAFT`, later ones get their own id.
    created: u32,
    /// False once a test asks for a draft Graph reports with no sender.
    sender: bool,
    reads: Vec<String>,
    sends: Vec<String>,
    /// When set, the POST to `/send` answers this status and body instead of `202`.
    send_refusal: Option<(u16, Value)>,
    /// When set, the POST to `/send` is lost in transit: the caller sees a timeout.
    send_timeout: bool,
}

pub struct Routed {
    world: Mutex<World>,
}

impl Routed {
    pub fn new(body: &str) -> Self {
        Self {
            world: Mutex::new(World {
                subject: SUBJECT.to_owned(),
                body: body.to_owned(),
                content_type: "text".to_owned(),
                created: 0,
                sender: true,
                reads: Vec::new(),
                sends: Vec::new(),
                send_refusal: None,
                send_timeout: false,
            }),
        }
    }

    /// Makes Graph refuse the next POST to `/send` with `status` and `body`.
    pub fn refuse_send(&self, status: u16, body: Value) {
        self.world.lock().expect("world").send_refusal = Some((status, body));
    }

    /// Makes the next POST to `/send` time out after it was sent.
    pub fn time_out_send(&self) {
        self.world.lock().expect("world").send_timeout = true;
    }

    /// Changes the draft as Graph would hold a plain-text edit made elsewhere.
    pub fn set_body(&self, body: &str) {
        let mut world = self.world.lock().expect("world");
        world.body = body.to_owned();
        world.content_type = "text".to_owned();
    }

    /// Changes the draft as Graph would hold an HTML body made elsewhere.
    pub fn set_html(&self, body: &str) {
        let mut world = self.world.lock().expect("world");
        world.body = body.to_owned();
        world.content_type = "html".to_owned();
    }

    /// The subject Graph holds, as the last write left it.
    pub fn subject(&self) -> String {
        self.world.lock().expect("world").subject.clone()
    }

    /// The body Graph holds, as the last write left it.
    pub fn body(&self) -> String {
        self.world.lock().expect("world").body.clone()
    }

    /// Keeps the body and content type a write sent, as Graph stores them.
    fn store_body(&self, payload: &Value) {
        if let Some(content) = payload["body"]["content"].as_str() {
            let mut world = self.world.lock().expect("world");
            world.body = content.to_owned();
            world.content_type = payload["body"]["contentType"]
                .as_str()
                .unwrap_or("text")
                .to_owned();
        }
    }

    /// Stores a create and answers the id Graph would give it.
    fn store_create(&self, payload: &Value) -> String {
        let mut world = self.world.lock().expect("world");
        world.created += 1;
        if let Some(subject) = payload["subject"].as_str() {
            world.subject = subject.to_owned();
        }
        if let Some(content) = payload["body"]["content"].as_str() {
            world.body = content.to_owned();
            world.content_type = payload["body"]["contentType"]
                .as_str()
                .unwrap_or("text")
                .to_owned();
        }
        if world.created == 1 {
            DRAFT.to_owned()
        } else {
            format!("AAMk-draft-{}", world.created)
        }
    }

    /// Makes the fake report the draft with no sender, as Graph does for some drafts.
    pub fn without_sender(&self) {
        self.world.lock().expect("world").sender = false;
    }

    pub fn reads(&self) -> Vec<String> {
        self.world.lock().expect("world").reads.clone()
    }

    pub fn sends(&self) -> Vec<String> {
        self.world.lock().expect("world").sends.clone()
    }

    fn message(&self) -> Value {
        self.message_with_id(DRAFT)
    }

    fn message_with_id(&self, id: &str) -> Value {
        let world = self.world.lock().expect("world");
        let from = if world.sender {
            json!({"emailAddress": {"address": OWNER}})
        } else {
            Value::Null
        };
        json!({
            "id": id,
            "subject": world.subject,
            "from": from,
            "toRecipients": [{"emailAddress": {"address": TO}}],
            "ccRecipients": [],
            "isDraft": true,
            "hasAttachments": false,
            "body": {"contentType": world.content_type, "content": world.body},
        })
    }
}

fn answer(status: u16, body: Value) -> HttpResponse {
    HttpResponse {
        status,
        body: body.to_string(),
    }
}

fn unexpected(url: &str) -> MailError {
    MailError::BadResponse(Redacted::new(format!("the fake has no answer for {url}")))
}

#[async_trait]
impl HttpTransport for Routed {
    /// The token endpoint: a refresh of the send token answers with the send
    /// access token, and a refresh of the login answers with the login one.
    async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<HttpResponse, MailError> {
        if !url.contains("/oauth2/v2.0/token") {
            return Err(unexpected(url));
        }
        let refresh = form
            .iter()
            .find(|(name, _)| *name == "refresh_token")
            .map_or("", |(_, value)| *value);
        let (access, next) = if refresh == SEND_REFRESH {
            (SEND_ACCESS, SEND_REFRESH)
        } else {
            (LOGIN_ACCESS, LOGIN_REFRESH)
        };
        Ok(answer(
            200,
            json!({"access_token": access, "refresh_token": next, "expires_in": 3600}),
        ))
    }

    async fn get_json(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError> {
        if url.contains("/attachments") {
            return Ok(answer(200, json!({"value": []})));
        }
        if url.contains(&format!("/me/messages/{DRAFT}")) {
            self.world
                .lock()
                .expect("world")
                .reads
                .push(bearer.to_owned());
            return Ok(answer(200, self.message()));
        }
        Err(unexpected(url))
    }

    async fn post_json(
        &self,
        url: &str,
        _bearer: &str,
        body: &Value,
    ) -> Result<HttpResponse, MailError> {
        if url.ends_with("/me/messages") {
            let id = self.store_create(body);
            return Ok(answer(201, self.message_with_id(&id)));
        }
        Err(unexpected(url))
    }

    async fn post_action(&self, url: &str, bearer: &str) -> Result<HttpResponse, MailError> {
        if url.ends_with("/send") {
            let mut world = self.world.lock().expect("world");
            world.sends.push(bearer.to_owned());
            if world.send_timeout {
                return Err(MailError::Transport(Redacted::new("the request timed out")));
            }
            if let Some((status, body)) = world.send_refusal.clone() {
                return Ok(answer(status, body));
            }
            return Ok(answer(202, Value::Null));
        }
        Err(unexpected(url))
    }

    async fn patch_json(
        &self,
        url: &str,
        _bearer: &str,
        body: &Value,
    ) -> Result<HttpResponse, MailError> {
        if url.contains(&format!("/me/messages/{DRAFT}")) {
            if let Some(subject) = body["subject"].as_str() {
                self.world.lock().expect("world").subject = subject.to_owned();
            }
            self.store_body(body);
            return Ok(answer(200, self.message()));
        }
        Err(unexpected(url))
    }

    async fn put_bytes(
        &self,
        url: &str,
        _content_type: &str,
        _bytes: &[u8],
    ) -> Result<HttpResponse, MailError> {
        Err(unexpected(url))
    }

    async fn put_range(
        &self,
        url: &str,
        _content_range: &str,
        _bytes: Vec<u8>,
    ) -> Result<HttpResponse, MailError> {
        Err(unexpected(url))
    }

    async fn get_upload_status(&self, url: &str) -> Result<HttpResponse, MailError> {
        Err(unexpected(url))
    }
}

/// Saves `ACCOUNT` with its send grant and creates `DRAFT` through the product's
/// own path, so the ledger owns it.
pub async fn seed_owner(store: &AccountStore, ledger: &Ledger, transport: &dyn HttpTransport) {
    let login = Tokens::new(LOGIN_ACCESS, Some(Secret::new(LOGIN_REFRESH)), 3600);
    let send = Tokens::new(SEND_ACCESS, Some(Secret::new(SEND_REFRESH)), 3600);
    let owner = Account::new(ACCOUNT, Tenant::Consumers, OWNER, login).with_send(send);
    store.save(&owner).expect("save");

    let input = DraftInput {
        to: vec![TO.to_owned()],
        cc: Vec::new(),
        subject: SUBJECT.to_owned(),
        body: DraftBody::Text(BODY.to_owned()),
    };
    GraphClient::new(transport, LOGIN_ACCESS, ledger, ACCOUNT)
        .create_draft(&input)
        .await
        .expect("draft");
}

/// Puts `address` in the address book, as Alfred does before a draft names it.
pub fn add_to_book(chat: &ChatStore, address: &str) {
    let alfred = chat.alfred_id().expect("alfred");
    let actor = SeatActor {
        agent_id: alfred,
        role_id: "alfred".to_string(),
        project_id: None,
        conversation_id: String::new(),
        run_id: String::new(),
    };
    chat.save_contact(
        &actor,
        None,
        ContactFields {
            name: address.to_string(),
            aliases: None,
            addresses: Some(vec![address.to_string()]),
            notes: None,
        },
    )
    .expect("contact");
}

/// One host: its store, chat, ledger and the fake Graph they talk to. The
/// `TestDir` lives as long as the host, so the files outlive every service.
pub struct Host {
    _dir: TestDir,
    pub world: Arc<Routed>,
    transport: Arc<dyn HttpTransport>,
    store: Arc<AccountStore>,
    pub chat: Arc<ChatStore>,
    ledger: Ledger,
    accounts: PathBuf,
}

impl Host {
    /// A host with `ACCOUNT` signed in with sending granted, and one draft that
    /// `ACCOUNT` made through the product's own path, so the ledger owns it.
    pub async fn start(name: &str) -> Self {
        let dir = TestDir::new(name);
        let accounts = dir.join("data").join("mail").join("accounts");
        let key = FileKey::new(dir.join("key"));
        let store = Arc::new(AccountStore::open(&accounts, &key).expect("store"));
        let chat = Arc::new(ChatStore::in_memory().expect("chat"));
        add_to_book(&chat, TO);
        let ledger = Ledger::in_memory().expect("ledger");
        let world = Arc::new(Routed::new(BODY));
        let transport: Arc<dyn HttpTransport> = world.clone();

        seed_owner(&store, &ledger, transport.as_ref()).await;

        Self {
            _dir: dir,
            world,
            transport,
            store,
            chat,
            ledger,
            accounts,
        }
    }

    /// A send service over this host's state. A second one, built the same way,
    /// stands for a restarted host.
    pub fn service(&self, window: Duration, per_hour: u32) -> MailSends {
        MailSends::new(
            Arc::clone(&self.transport),
            Arc::clone(&self.store),
            Some("app".to_owned()),
            self.ledger.clone(),
            &self.accounts,
            Arc::clone(&self.chat),
            SendPolicy { per_hour, window },
        )
    }

    /// The `mail.*` tools over this host's state, sharing the given send service
    /// the way the host wires them, so a draft change settles that service's
    /// open questions.
    pub fn tools(&self, sends: MailSends) -> MailTools {
        let data = self.accounts.ancestors().nth(2).expect("data dir");
        MailTools::new(
            Arc::clone(&self.transport),
            Arc::clone(&self.store),
            Some("app".to_owned()),
            data,
            Arc::clone(&self.chat),
            &data.join("workspaces"),
            sends,
        )
        .expect("mail tools")
    }

    /// Takes the send grant away the way `host mail disable-send` does.
    pub fn disable_send(&self) {
        let account = self
            .store
            .load(ACCOUNT)
            .expect("load")
            .expect("saved account");
        self.store.save(&account.without_send()).expect("save");
    }

    pub fn audit_text(&self) -> String {
        std::fs::read_to_string(self.accounts.join("audit.jsonl")).unwrap_or_default()
    }

    /// The audit line follows the state change by a moment, so wait for it
    /// instead of racing it.
    pub async fn expect_audit(&self, needle: &str) {
        for _ in 0..300 {
            if self.audit_text().contains(needle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the audit never recorded {needle}");
    }

    /// Decides the approval as the device does in the chat.
    pub fn decide(&self, approval_id: &str, decision: Decision) {
        self.chat
            .decide_approval(approval_id, decision, "device-1", "device-1")
            .expect("the device decides");
    }
}

/// Asks the owner about sending the draft and returns the approval to answer.
pub async fn ask(service: &MailSends) -> String {
    match service.send(ACCOUNT, DRAFT, None).await.expect("ask") {
        SendStep::Awaiting { approval_id, .. } => approval_id,
        other => panic!("expected an approval request, got {other:?}"),
    }
}

/// Presents the approval again and returns the id of the scheduled send.
pub async fn scheduled(service: &MailSends, approval_id: &str) -> String {
    match service
        .send(ACCOUNT, DRAFT, Some(approval_id))
        .await
        .expect("schedule")
    {
        SendStep::Scheduled { send_id, .. } => send_id,
        other => panic!("expected a scheduled send, got {other:?}"),
    }
}

/// Waits until the send leaves `pending`, which happens when its window ends
/// or when it is cancelled.
pub async fn settle(service: &MailSends, send_id: &str) -> &'static str {
    for _ in 0..300 {
        let state = service.status(send_id).expect("status").state.public();
        if state != "pending" {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the send never left pending");
}

/// Approves and schedules one send, and waits for it to leave the window.
pub async fn approved_and_sent(host: &Host, service: &MailSends) -> String {
    let approval = ask(service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(service, &approval).await;
    assert_eq!(settle(service, &send_id).await, "sent");
    send_id
}
