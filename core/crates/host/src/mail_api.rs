//! What `POST /v1/mail/login/start`, `GET /v1/mail/login/status`,
//! `GET /v1/mail/accounts` (MAIL1C §4) and the sending consent (MAIL2A) answer.
//! The routes are wired in hn-core-api; this module keeps the logins and never
//! returns a token.
//!
//! The device code stays here. The client gets the user code and the page to
//! type it on, and the status of its login. A background task polls Microsoft
//! until the user answers, then saves the account encrypted. A sending consent
//! is a second device login, with its own scopes, that adds send tokens to an
//! account that already exists; a normal login never carries them.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use hn_core_chat::{new_ulid, Approval, ChatError, ChatStore, NewMailCard};
use hn_core_mail::auth::{
    poll_device_login, start_device_login, start_send_consent, DeviceLogin, Tenant,
};
use hn_core_mail::graph::fetch_upn;
use hn_core_mail::{Account, AccountStore, HttpTransport, MailError, Redacted};
use serde::Serialize;

use crate::mail_send::MailSends;
use crate::mail_tools::MailTools;

/// Where a login stands: the shape the API serves, so no mapping is needed.
pub use hn_core_api::MailLoginStatus as LoginStatus;

/// What a device login grants: the account itself, or the send permission on an
/// account that already exists (MAIL2A).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Login,
    Send,
}

/// The answer to a login start. Nothing here lets anyone else finish the login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoginStart {
    pub login_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// One login this host has started, and where it stands.
struct Login {
    login_id: String,
    account: String,
    purpose: Purpose,
    status: LoginStatus,
}

/// One account in the listing: what `host mail accounts` prints, as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountSummary {
    pub label: String,
    pub tenant: Tenant,
    pub upn: String,
    pub send_enabled: bool,
}

/// The logins in progress and finished on this host, and the send service when
/// the host has one.
#[derive(Clone)]
pub struct MailLogins {
    inner: Arc<Inner>,
}

struct Inner {
    transport: Arc<dyn HttpTransport>,
    store: Arc<AccountStore>,
    client_id: Option<String>,
    chat: Arc<ChatStore>,
    logins: Mutex<Vec<Login>>,
    sends: Option<MailSends>,
    /// The `mail.*` tools that apply an approved batch. Set once, by the host's startup.
    tools: OnceLock<Arc<MailTools>>,
}

impl MailLogins {
    /// `client_id` is `None` when the host has no app registration: every
    /// login is then refused with `mail_not_configured`. `chat` holds the
    /// sign-in cards a login shows in a conversation. A logins-only surface has
    /// no send service, so every send is refused.
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        store: Arc<AccountStore>,
        client_id: Option<String>,
        chat: Arc<ChatStore>,
    ) -> Self {
        Self::build(transport, store, client_id, chat, None)
    }

    /// The logins and the sends of a host, sharing one account store and transport.
    pub fn with_sends(
        transport: Arc<dyn HttpTransport>,
        store: Arc<AccountStore>,
        client_id: Option<String>,
        chat: Arc<ChatStore>,
        sends: MailSends,
    ) -> Self {
        Self::build(transport, store, client_id, chat, Some(sends))
    }

    fn build(
        transport: Arc<dyn HttpTransport>,
        store: Arc<AccountStore>,
        client_id: Option<String>,
        chat: Arc<ChatStore>,
        sends: Option<MailSends>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                transport,
                store,
                client_id,
                chat,
                logins: Mutex::new(Vec::new()),
                sends,
                tools: OnceLock::new(),
            }),
        }
    }

    /// Hands over the tools that apply an approved mail batch. The first attach stands;
    /// until one happens, a decided batch is left as it is.
    pub fn attach_tools(&self, tools: Arc<MailTools>) {
        let _ = self.inner.tools.set(tools);
    }

    /// Starts applying an approval the operator just decided, if it is a mail batch.
    pub(crate) fn decided(&self, approval: &Approval) {
        if let Some(tools) = self.inner.tools.get() {
            tools.decided_in_background(approval.clone());
        }
    }

    /// The send service. `NotConfigured` when the host was built without one.
    pub fn sends(&self) -> Result<&MailSends, MailError> {
        self.inner.sends.as_ref().ok_or(MailError::NotConfigured)
    }

    /// Starts a device login for `account` and returns at once; the answer
    /// arrives through [`MailLogins::status`]. With a `conversation_id`, a
    /// sign-in card shows the code there until the login ends, however it ends.
    pub async fn start(
        &self,
        account: &str,
        tenant: Tenant,
        conversation_id: Option<&str>,
    ) -> Result<LoginStart, MailError> {
        self.begin(Purpose::Login, account, tenant, conversation_id)
            .await
    }

    /// Starts the sending consent of an account that is already saved. The
    /// tenant is the account's own; the consent never changes it.
    pub async fn start_send(&self, account: &str) -> Result<LoginStart, MailError> {
        let saved = self.saved(account)?;
        self.begin(Purpose::Send, account, saved.tenant, None).await
    }

    async fn begin(
        &self,
        purpose: Purpose,
        account: &str,
        tenant: Tenant,
        conversation_id: Option<&str>,
    ) -> Result<LoginStart, MailError> {
        let client_id = self
            .inner
            .client_id
            .clone()
            .ok_or(MailError::NotConfigured)?;
        if let Some(conversation) = conversation_id {
            self.inner
                .chat
                .conversation(conversation)
                .map_err(chat_error)?;
        }
        let transport = self.inner.transport.as_ref();
        let device = match purpose {
            Purpose::Login => start_device_login(transport, &client_id, account, tenant).await?,
            Purpose::Send => start_send_consent(transport, &client_id, account, tenant).await?,
        };
        let start = LoginStart {
            login_id: new_ulid(),
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
            expires_in: device.expires_in,
            interval: device.interval,
        };
        if let Some(conversation) = conversation_id {
            self.inner
                .chat
                .open_mail_card(NewMailCard {
                    conversation_id: conversation,
                    login_id: &start.login_id,
                    account,
                    user_code: &start.user_code,
                    verification_uri: &start.verification_uri,
                    expires_in: start.expires_in,
                })
                .map_err(chat_error)?;
        }
        self.record(&start.login_id, account, purpose, LoginStatus::Pending);

        let logins = self.clone();
        let login_id = start.login_id.clone();
        let label = account.to_owned();
        tokio::spawn(async move {
            let status = logins
                .finish(purpose, &client_id, &label, tenant, &device)
                .await;
            // The card goes before the status is published, so whoever sees the
            // final status finds no card left behind.
            if logins.inner.chat.close_mail_card(&login_id).is_err() {
                eprintln!("mail: the sign-in card could not be taken away");
            }
            logins.record(&login_id, &label, purpose, status);
        });
        Ok(start)
    }

    /// `mail_invalid_input` for a login id this host never issued.
    pub fn status(&self, login_id: &str) -> Result<LoginStatus, MailError> {
        self.logins()
            .iter()
            .find(|login| login.login_id == login_id)
            .map(|login| login.status.clone())
            .ok_or_else(unknown_login)
    }

    /// The status of the latest login started for `account`: what the user
    /// sees while they finish that sign-in. `mail_invalid_input` when none.
    pub fn status_of(&self, account: &str) -> Result<LoginStatus, MailError> {
        self.latest(account, Purpose::Login)
    }

    /// The status of the latest sending consent started for `account`.
    pub fn send_status_of(&self, account: &str) -> Result<LoginStatus, MailError> {
        self.latest(account, Purpose::Send)
    }

    fn latest(&self, account: &str, purpose: Purpose) -> Result<LoginStatus, MailError> {
        self.logins()
            .iter()
            .rev()
            .find(|login| login.account == account && login.purpose == purpose)
            .map(|login| login.status.clone())
            .ok_or_else(unknown_login)
    }

    /// The accounts saved on this host. Tokens are never read into the listing.
    pub fn accounts(&self) -> Result<Vec<AccountSummary>, MailError> {
        Ok(self
            .inner
            .store
            .all()?
            .into_iter()
            .map(|account| AccountSummary {
                send_enabled: account.send_enabled(),
                label: account.label,
                tenant: account.tenant,
                upn: account.upn,
            })
            .collect())
    }

    /// Deletes the send tokens of `account`. The login stays; sending stops at once.
    pub fn disable_send(&self, account: &str) -> Result<(), MailError> {
        let saved = self.saved(account)?;
        self.inner.store.save(&saved.without_send())
    }

    /// The saved account with this label. `mail_invalid_input` when there is none.
    pub(crate) fn saved(&self, account: &str) -> Result<Account, MailError> {
        self.inner
            .store
            .load(account)?
            .ok_or_else(|| MailError::InvalidInput(Redacted::new("no account with that label")))
    }

    /// Waits for the user's answer, then saves the account. Every failure ends
    /// as a status with a stable code, never as a panic or a leaked token.
    async fn finish(
        &self,
        purpose: Purpose,
        client_id: &str,
        label: &str,
        tenant: Tenant,
        device: &DeviceLogin,
    ) -> LoginStatus {
        let transport = self.inner.transport.as_ref();
        let tokens = match poll_device_login(transport, client_id, tenant, device).await {
            Ok(tokens) => tokens,
            Err(MailError::LoginDenied) => return LoginStatus::Denied,
            Err(MailError::LoginExpired) => return LoginStatus::Expired,
            Err(error) => {
                return LoginStatus::Error {
                    code: error.code().to_owned(),
                }
            }
        };
        let saved = async {
            match purpose {
                Purpose::Login => {
                    let upn = fetch_upn(transport, tokens.access_token()).await?;
                    let account = Account::new(label, tenant, upn.clone(), tokens);
                    self.inner.store.save(&account)?;
                    Ok::<String, MailError>(upn)
                }
                Purpose::Send => {
                    let account = self.saved(label)?;
                    let upn = account.upn.clone();
                    self.inner.store.save(&account.with_send(tokens))?;
                    Ok(upn)
                }
            }
        };
        match saved.await {
            Ok(upn) => LoginStatus::Ok {
                label: label.to_owned(),
                upn,
            },
            Err(error) => LoginStatus::Error {
                code: error.code().to_owned(),
            },
        }
    }

    /// Records the status of a login, starting it on first sight.
    fn record(&self, login_id: &str, account: &str, purpose: Purpose, status: LoginStatus) {
        let mut logins = self.logins();
        match logins.iter_mut().find(|login| login.login_id == login_id) {
            Some(login) => login.status = status,
            None => logins.push(Login {
                login_id: login_id.to_owned(),
                account: account.to_owned(),
                purpose,
                status,
            }),
        }
    }

    fn logins(&self) -> MutexGuard<'_, Vec<Login>> {
        self.inner
            .logins
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn unknown_login() -> MailError {
    MailError::InvalidInput(Redacted::new("unknown login"))
}

/// The chat store's refusal as a mail error: an unknown conversation is the
/// caller's input, anything else is the store failing.
fn chat_error(error: ChatError) -> MailError {
    match error {
        ChatError::NoSuchConversation => {
            MailError::InvalidInput(Redacted::new("unknown conversation"))
        }
        other => MailError::Store(Redacted::new(other.to_string())),
    }
}
