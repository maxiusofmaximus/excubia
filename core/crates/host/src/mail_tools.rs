//! The `mail.*` tools a chat turn may call (MAIL1D §3): list the accounts, create,
//! update and read a draft, attach a file to a draft; and read the inbox (MAIL-LECTURA):
//! list a folder, read one received message, triage a folder (`mail_triage`). Each call
//! refreshes the token of the account it names, saves the rotated tokens and then talks
//! to Graph. Nothing here sends mail or changes a received message: the surface has no
//! send, and the ledger keeps what this host made, so a draft is only ever changed by the
//! host that made it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use hn_core_chat::{ChatStore, SeatActor, ToolError, ToolErrorKind, ToolReply};
use hn_core_mail::attach::{attach_file, AttachContext};
use hn_core_mail::auth;
use hn_core_mail::{
    agent_html, sanitize_html, sanitize_text, AccountStore, ActionLog, AuditLog, DraftBody,
    DraftInput, DraftMeta, GraphClient, HttpTransport, Ledger, MailError, PathPolicy, Policy,
    ReadRecord, Redacted, DEFAULT_MAX_ATTACHMENT_BYTES,
};
use hn_core_seat::{
    BodyFormat, DraftFields, DraftRef, InboxQuery, MailAttach, MailCall, MessageRef,
};
use serde_json::{json, Value};

use crate::mail_approval::ChatApprovalGate;
use crate::mail_cli::ACCOUNTS_DIR;
use crate::mail_move_pick::ambiguous_reply;
use crate::mail_read;
use crate::mail_routes::{message_of, parts};
use crate::mail_send::MailSends;
use crate::mail_shadow::Shadow;
use crate::team_errors::tool_chat_error;

/// The ledger of what this host created, in the data dir and never in memory:
/// ownership must survive a restart, or a draft could no longer be changed.
pub const LEDGER_FILE: &str = "mail/ledger.sqlite";
/// Where the private copy of an approved file waits for its upload.
pub const SCRATCH_DIR: &str = "mail/scratch";

/// Opens the ledger the drafts and the sends share.
pub fn open_ledger(data_dir: &Path) -> Result<Ledger, MailError> {
    let ledger_path = data_dir.join(LEDGER_FILE);
    if let Some(parent) = ledger_path.parent() {
        create_dir(parent)?;
    }
    Ledger::open(&ledger_path)
}

pub struct MailTools {
    transport: Arc<dyn HttpTransport>,
    pub(crate) store: Arc<AccountStore>,
    client_id: Option<String>,
    ledger: Ledger,
    pub(crate) audit: AuditLog,
    /// The moves the host made, for `mail.undo`. Beside the audit log, in the accounts folder.
    pub(crate) actions: ActionLog,
    policy: PathPolicy,
    scratch: PathBuf,
    pub(crate) chat: Arc<ChatStore>,
    workspaces_root: PathBuf,
    sends: MailSends,
    /// The triage sidecar in shadow mode. Off unless the host was started with `--mail-sidecar`.
    pub(crate) shadow: Option<Arc<Shadow>>,
}

impl MailTools {
    /// The tools of a host, with the account store and registration of
    /// `mail_routes`. `workspaces_root` is the folder a file may be attached
    /// from without asking. `sends` is the host's send service: a change to a
    /// draft settles its open question there.
    pub fn open(
        data_dir: &Path,
        chat: Arc<ChatStore>,
        workspaces_root: &Path,
        sends: MailSends,
    ) -> Result<Self, MailError> {
        let (store, client_id, transport) = parts(data_dir)?;
        Self::new(
            Arc::new(transport),
            Arc::new(store),
            client_id,
            data_dir,
            chat,
            workspaces_root,
            sends,
        )
    }

    pub fn new(
        transport: Arc<dyn HttpTransport>,
        store: Arc<AccountStore>,
        client_id: Option<String>,
        data_dir: &Path,
        chat: Arc<ChatStore>,
        workspaces_root: &Path,
        sends: MailSends,
    ) -> Result<Self, MailError> {
        let scratch = data_dir.join(SCRATCH_DIR);
        create_dir(&scratch)?;
        create_dir(workspaces_root)?;
        Ok(Self {
            transport,
            store,
            client_id,
            ledger: sends.ledger(),
            audit: AuditLog::new(&data_dir.join(ACCOUNTS_DIR)),
            actions: ActionLog::new(&data_dir.join(ACCOUNTS_DIR)),
            policy: PathPolicy::new(workspaces_root, DEFAULT_MAX_ATTACHMENT_BYTES)?,
            scratch,
            chat,
            workspaces_root: workspaces_root.to_path_buf(),
            sends,
            shadow: None,
        })
    }

    /// Adds the triage sidecar in shadow mode: each triage then also feeds the shadow log.
    pub fn with_shadow(mut self, shadow: Arc<Shadow>) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Runs one `mail.*` call for `actor`. Failures carry the stable code and a
    /// fixed message, never the address, path or token behind them.
    pub async fn call(&self, actor: &SeatActor, call: MailCall) -> Result<ToolReply, ToolError> {
        match &call {
            MailCall::DraftCreate(fields) => self.check_recipients(actor, fields)?,
            MailCall::DraftUpdate(update) => self.check_recipients(actor, &update.fields)?,
            _ => {}
        }
        let outcome = match call {
            MailCall::Accounts => self.accounts(),
            MailCall::DraftCreate(fields) => self.create(actor, &fields).await,
            MailCall::DraftUpdate(update) => {
                self.update(actor, &update.draft_id, &update.fields).await
            }
            MailCall::DraftGet(reference) => self.get(&reference).await,
            MailCall::AttachFile(file) => self.attach(actor, &file).await,
            MailCall::InboxList(query) => self.inbox(&query).await,
            MailCall::MessageGet(reference) => self.message(&reference).await,
            MailCall::Triage(query) => self.triage(&query).await,
            MailCall::Folders(query) => self.folders(&query).await,
            MailCall::Organize(call) => self.organize(actor, call).await,
        };
        outcome
            .map(|result| ToolReply { result })
            .map_err(tool_error)
    }

    /// A draft reaches only addresses of the address book, or ones the user wrote
    /// literally in this conversation (F3B), which the book then keeps. The check runs
    /// before a token is refreshed or Graph is called, and the refusal names no address.
    fn check_recipients(&self, actor: &SeatActor, fields: &DraftFields) -> Result<(), ToolError> {
        let recipients: Vec<String> = fields.to.iter().chain(&fields.cc).cloned().collect();
        let refused = self
            .chat
            .admit_recipients(actor, &recipients)
            .map_err(tool_chat_error)?;
        if refused.is_empty() {
            return Ok(());
        }
        Err(ToolError::invalid(
            "unknown_recipient",
            "to",
            "an address is not in the address book and the user did not write it in this conversation: find the contact with contact.find, or ask the user and save it with contact.save",
        ))
    }

    fn accounts(&self) -> Result<Value, MailError> {
        let accounts = self.store.all()?;
        let listed: Vec<Value> = accounts
            .iter()
            .map(|account| {
                json!({
                    "label": account.label,
                    "tenant": account.tenant.as_str(),
                    "upn": account.upn,
                })
            })
            .collect();
        Ok(json!({ "ok": true, "accounts": listed }))
    }

    async fn create(&self, actor: &SeatActor, fields: &DraftFields) -> Result<Value, MailError> {
        let draft = input(fields, HtmlOrigin::Owner)?;
        let access = self.session(&fields.account).await?;
        let client = self.client(&access, &fields.account);
        let meta = client.create_draft(&draft).await?;
        self.show(actor, &fields.account, &meta.id, &draft)?;
        Ok(draft_json(&meta))
    }

    async fn update(
        &self,
        actor: &SeatActor,
        draft_id: &str,
        fields: &DraftFields,
    ) -> Result<Value, MailError> {
        let draft = input(fields, HtmlOrigin::Owner)?;
        let access = self.session(&fields.account).await?;
        let client = self.client(&access, &fields.account);
        let meta = client.update_draft(draft_id, &draft).await?;
        self.sends.draft_changed(&fields.account, draft_id)?;
        self.show(actor, &fields.account, draft_id, &draft)?;
        Ok(draft_json(&meta))
    }

    /// The chat card of a draft the turn wrote (MAILUX-HOST §A). A turn outside a
    /// conversation has no card to write.
    fn show(
        &self,
        actor: &SeatActor,
        account: &str,
        draft_id: &str,
        draft: &DraftInput,
    ) -> Result<(), MailError> {
        let conversation =
            (!actor.conversation_id.is_empty()).then_some(actor.conversation_id.as_str());
        self.sends.show_draft(
            conversation,
            account,
            draft_id,
            &draft.to,
            &draft.subject,
            &draft.body,
        )
    }

    async fn get(&self, reference: &DraftRef) -> Result<Value, MailError> {
        let access = self.session(&reference.account).await?;
        let client = self.client(&access, &reference.account);
        let meta = client.get_draft_meta(&reference.draft_id).await?;
        Ok(draft_json(&meta))
    }

    async fn attach(&self, actor: &SeatActor, file: &MailAttach) -> Result<Value, MailError> {
        let access = self.session(&file.account).await?;
        let client = self.client(&access, &file.account);
        let gate = self.gate(&actor.conversation_id);
        let ctx = AttachContext {
            client: &client,
            account: &file.account,
            policy: &self.policy,
            approval: &gate,
            audit: &self.audit,
            scratch: &self.scratch,
        };
        let attached = attach_file(&ctx, &file.draft_id, Path::new(&file.path)).await?;
        self.sends.draft_changed(&file.account, &file.draft_id)?;
        Ok(json!({
            "ok": true,
            "attachment": {
                "id": attached.attachment_id,
                "name": attached.name,
                "size": attached.size,
                "sha256": attached.sha256,
            },
        }))
    }

    /// One GET for the listing, then the read is audited by account and tool only.
    async fn inbox(&self, query: &InboxQuery) -> Result<Value, MailError> {
        let access = self.session(&query.account).await?;
        let client = self.client(&access, &query.account);
        let items = client
            .list_inbox(
                query.folder.graph_name(),
                query.limit,
                query.unread_only,
                query.from.as_deref(),
            )
            .await?;
        self.audit
            .append(&ReadRecord::new(&query.account, "mail.inbox_list", None))?;
        Ok(mail_read::inbox_reply(&items))
    }

    /// Reads one received message and its checks, then audits the read by its id.
    async fn message(&self, reference: &MessageRef) -> Result<Value, MailError> {
        let access = self.session(&reference.account).await?;
        let client = self.client(&access, &reference.account);
        let message = client.read_message(&reference.message_id).await?;
        self.audit.append(&ReadRecord::new(
            &reference.account,
            "mail.message_get",
            Some(&reference.message_id),
        ))?;
        Ok(mail_read::message_reply(&message))
    }

    /// Refreshes the account's token for this call and saves the rotated pair,
    /// so the next call starts from the newest refresh token.
    pub(crate) async fn session(&self, label: &str) -> Result<String, MailError> {
        let client_id = self.client_id.as_deref().ok_or(MailError::NotConfigured)?;
        let account = self.store.load(label)?.ok_or_else(|| {
            MailError::InvalidInput(Redacted::new("no account with that label is signed in"))
        })?;
        let refresh = account
            .tokens
            .refresh_token()
            .ok_or(MailError::AuthRequired)?;
        let tokens =
            auth::refresh(self.transport.as_ref(), client_id, account.tenant, refresh).await?;
        let access = tokens.access_token().to_owned();
        self.store.save(&account.renewed(tokens))?;
        Ok(access)
    }

    /// The ledger this host's drafts are recorded in. The send service shares it,
    /// so a draft's owner and its sends are read from the same rows.
    pub fn ledger(&self) -> Ledger {
        self.ledger.clone()
    }

    pub(crate) fn client<'a>(&'a self, access: &'a str, account: &'a str) -> GraphClient<'a> {
        GraphClient::new(self.transport.as_ref(), access, &self.ledger, account)
    }

    /// The operator's question for a file outside the workspace, asked in the
    /// conversation of the turn that wants it.
    fn gate(&self, conversation_id: &str) -> ChatApprovalGate<'_> {
        let gate = ChatApprovalGate::new(
            self.chat.as_ref(),
            self.workspaces_root.display().to_string(),
        );
        if conversation_id.is_empty() {
            gate
        } else {
            gate.in_conversation(conversation_id)
        }
    }
}

fn create_dir(path: &Path) -> Result<(), MailError> {
    std::fs::create_dir_all(path)
        .map_err(|error| MailError::Store(Redacted::new(error.to_string())))
}

/// Where an HTML body comes from. The owner's HTML is cleaned; an agent template is
/// the host's own markup and reaches Graph as it was drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlOrigin {
    Owner,
    AgentTemplate,
}

pub(crate) fn input(fields: &DraftFields, origin: HtmlOrigin) -> Result<DraftInput, MailError> {
    let subject = sanitize_text(&fields.subject, Policy::Outgoing)?;
    let body = match fields.format {
        BodyFormat::Text => DraftBody::Text(sanitize_text(&fields.body, Policy::Outgoing)?.text),
        BodyFormat::Html => DraftBody::Html(match origin {
            HtmlOrigin::Owner => sanitize_html(&fields.body)?,
            HtmlOrigin::AgentTemplate => fields.body.clone(),
        }),
        BodyFormat::Agent => DraftBody::Html(agent_html(&fields.subject, &fields.body)?),
    };
    Ok(DraftInput {
        to: fields.to.clone(),
        cc: fields.cc.clone(),
        subject: subject.text,
        body,
    })
}

/// The metadata the tools return: no body, the draft's own recipients and the
/// names of its attachments.
fn draft_json(meta: &DraftMeta) -> Value {
    json!({
        "ok": true,
        "draft": {
            "id": meta.id,
            "subject": meta.subject,
            "to": meta.to,
            "cc": meta.cc,
            "is_draft": meta.is_draft,
            "attachments": meta.attachment_names,
        },
    })
}

/// Refusals the caller can fix by changing its request; everything else is the
/// host or Graph failing.
fn tool_error(error: MailError) -> ToolError {
    let code = error.code();
    let kind = if error.is_refusal() {
        ToolErrorKind::Invalid
    } else {
        ToolErrorKind::Unavailable
    };
    let (field, message) = ambiguous_reply(&error).unwrap_or((None, message_of(code).to_owned()));
    ToolError {
        kind,
        code,
        field,
        message,
    }
}
