//! Creating an approval (CORE9-F). The rules and the scorer answer before the
//! write lock is taken, so a scorer that takes its 200 ms never holds the
//! database. The write then checks again what the answer depended on, and asks
//! again when the rules, the mode or the seat moved meanwhile.

use rusqlite::{params, Connection, Transaction};
use serde_json::Value;

use crate::clock::iso;
use crate::error::ChatError;
use crate::id;
use crate::model_approval::{
    max_ttl_seconds, Approval, ApprovalStatus, NewApproval, MAIL_ACTIONS,
    MAX_PENDING_PER_CONVERSATION,
};
use crate::model_mode::ApprovalMode;
use crate::policy::Category;
use crate::scorer::Judgement;

use super::agenda::delete_key;
use super::approval_level::ApprovalLevel;
use super::approval_mode;
use super::approval_policy::{Assessment, Placement, SCORER_ACTOR};
use super::approval_rules::{
    approval_by_id, digest_holder, frame_of, pending_in_conversation, project_of_seat,
    resolve_context, write_audit, ApprovalContext, Decider,
};
use super::approval_settle;
use super::company_id;

/// How many times a request is judged when the rules moved while the scorer
/// answered. The rules change by the operator's hand, so a retry almost always
/// lands; a run of changes is refused rather than looped on.
const MAX_JUDGEMENTS: usize = 3;

/// What a new approval depends on, read before the scorer is asked. The write
/// reads it again and places the approval only when it is unchanged.
#[derive(PartialEq)]
struct Basis {
    context: ApprovalContext,
    project_id: Option<String>,
    mode: ApprovalMode,
    assessment: Assessment,
}

/// The read side of a creation: the request is already here, or it may be
/// placed on this basis.
enum Admission {
    /// The same seat asks again for an action it still waits on.
    Existing(String),
    Open(Basis),
}

/// The pending question that already asks to delete the same note or contact, whatever
/// the conversation. Such an object belongs to the company, so one question stands for
/// it. Each request carries its own digest, so the digest cannot find it.
fn pending_delete(connection: &Connection, new: &NewApproval) -> Result<Option<String>, ChatError> {
    let Some(key) = delete_key(&new.tool) else {
        return Ok(None);
    };
    let Some(object) = new
        .payload_redacted
        .as_ref()
        .and_then(|payload| payload[key].as_str())
    else {
        return Ok(None);
    };
    let mut statement = connection.prepare(
        "SELECT id, payload_redacted FROM approval
          WHERE tool = ?1 AND status = 'pending' AND seat_id IS NULL",
    )?;
    let rows = statement.query_map([new.tool.as_str()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    for row in rows {
        let (approval_id, payload) = row?;
        let asks_for_object = payload
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|payload| payload[key].as_str() == Some(object));
        if asks_for_object {
            return Ok(Some(approval_id));
        }
    }
    Ok(None)
}

impl super::ChatStore {
    /// Create one approval in `pending` (or decided by the policy). The digest
    /// is the identity of the action: a digest that already exists is never
    /// approved twice.
    pub fn create_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        // A mail action is placed by the host with its own category; a caller cannot file one.
        if MAIL_ACTIONS.contains(&new.tool.as_str()) {
            return Err(ChatError::ApprovalScopeDenied);
        }
        self.create_placed(new, None)
    }

    /// Create an approval that reaches outside the workspace (MAIL1D §0). Its
    /// category is set by the host, whatever the seat and its project: the
    /// rules send it to the operator, so no agent can decide it.
    pub fn create_outside_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        self.create_placed(new, Some(Category::OutsideWorkspace))
    }

    /// Create the question for deleting user data, such as a note (F2). Forced to
    /// `DeleteUserData`, which no rule reaches: the operator's device decides it,
    /// never the lead.
    pub fn create_delete_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        self.expire_now()?;
        self.create_placed(new, Some(Category::DeleteUserData))
    }

    /// Create the question for sending one mail (MAIL2A). Forced to `MailSend`,
    /// which is never delegable: only a device with APPROVE can let it through.
    pub fn create_mail_send_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        self.create_mail_approval(new, Category::MailSend)
    }

    /// Create the question for a batch of mail moves (F2b-1). Forced to
    /// `MailOrganize`: one approval covers the whole batch, and only a device
    /// with APPROVE can let it through.
    pub fn create_mail_organize_approval(&self, new: NewApproval) -> Result<Approval, ChatError> {
        self.create_mail_approval(new, Category::MailOrganize)
    }

    fn create_mail_approval(
        &self,
        new: NewApproval,
        category: Category,
    ) -> Result<Approval, ChatError> {
        self.expire_now()?;
        self.hold_mail_digest(&new.action_digest)?;
        self.create_placed(new, Some(category))
    }

    /// Decides what an earlier mail question on the same content leaves behind
    /// (MAILUX-HOST §C). An expired one can no longer be answered, so it lets the
    /// content be asked again: its digest is renamed, which frees the unique index
    /// and keeps the digest in its audit lines. A denied one refuses the content
    /// until the draft changes. Anything else keeps the content held as before.
    fn hold_mail_digest(&self, digest: &str) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if let Some(holder) = digest_holder(&transaction, digest)? {
            if holder.status == ApprovalStatus::Expired.as_str() {
                transaction.execute(
                    "UPDATE approval SET action_digest = ?1 WHERE id = ?2",
                    params![format!("{digest}#expired:{}", holder.id), holder.id],
                )?;
            } else if holder.status == ApprovalStatus::Denied.as_str() {
                return Err(ChatError::ApprovalDigestRefused);
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// `reach` is the category the host forces on the action, when it knows
    /// one the payload cannot show. `None` leaves the category to the payload.
    fn create_placed(
        &self,
        new: NewApproval,
        reach: Option<Category>,
    ) -> Result<Approval, ChatError> {
        self.expire_now()?;
        for _ in 0..MAX_JUDGEMENTS {
            let basis = match self.read_admission(&new, reach)? {
                Admission::Existing(approval_id) => return self.approval(&approval_id),
                Admission::Open(basis) => basis,
            };
            let judged = self.judge(&basis.assessment, &new);
            if let Some(approval_id) = self.place(&new, &basis, judged, reach)? {
                return self.approval(&approval_id);
            }
        }
        Err(ChatError::ApprovalRaced)
    }

    fn read_admission(
        &self,
        new: &NewApproval,
        reach: Option<Category>,
    ) -> Result<Admission, ChatError> {
        let connection = self.lock();
        self.admit(&connection, new, reach)
    }

    /// What a creation depends on, read from `connection`. A digest held by the
    /// same seat's pending approval answers with that approval; a digest held by
    /// anyone else, or a full conversation, is refused.
    fn admit(
        &self,
        connection: &Connection,
        new: &NewApproval,
        reach: Option<Category>,
    ) -> Result<Admission, ChatError> {
        let context = resolve_context(connection, new)?;
        let seat = seat_of(&context, new);
        if let Some(holder) = digest_holder(connection, &new.action_digest)? {
            if holder.status == ApprovalStatus::Pending.as_str()
                && holder.seat_id.as_deref() == seat.as_deref()
            {
                return Ok(Admission::Existing(holder.id));
            }
            return Err(ChatError::ApprovalDigestReused);
        }
        if let Some(approval_id) = pending_delete(connection, new)? {
            return Ok(Admission::Existing(approval_id));
        }
        if pending_in_conversation(connection, context.conversation_id.as_deref())?
            >= MAX_PENDING_PER_CONVERSATION
        {
            return Err(ChatError::ApprovalLimit);
        }
        let assessment = self.assess(connection, new, seat.as_deref(), reach)?;
        let project_id = match seat.as_deref() {
            Some(seat) => project_of_seat(connection, seat)?,
            None => None,
        };
        let mode = approval_mode::effective(connection, project_id.as_deref())?;
        Ok(Admission::Open(Basis {
            context,
            project_id,
            mode,
            assessment,
        }))
    }

    /// Write the approval if the basis still holds. `None` when the rules, the
    /// mode or the seat moved since the basis was read: nothing is written and
    /// the request is judged again.
    fn place(
        &self,
        new: &NewApproval,
        basis: &Basis,
        judged: Option<(Judgement, bool)>,
        reach: Option<Category>,
    ) -> Result<Option<String>, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let fresh = match self.admit(&transaction, new, reach)? {
            Admission::Existing(approval_id) => return Ok(Some(approval_id)),
            Admission::Open(fresh) => fresh,
        };
        if fresh != *basis {
            return Ok(None);
        }
        let now_ms = self.now_ms();
        let now = iso(now_ms);
        let mut placement = Placement::of(&basis.assessment.verdict, basis.assessment.risk);
        if let Some((judgement, approves)) = judged {
            placement = placement.scored(judgement, approves);
        }
        let mut placement = placement.under(basis.mode);
        let ttl_ms = i64::from(new.ttl_seconds.clamp(1, max_ttl_seconds(reach))) * 1000;
        let expires_ms = now_ms + ttl_ms;
        approval_settle::route(
            &transaction,
            &mut placement,
            basis.project_id.as_deref(),
            basis.context.conversation_id.is_some(),
            now_ms,
            expires_ms,
        )?;
        let approval_id = id::new_id("apr");
        insert_approval(
            &transaction,
            &approval_id,
            new,
            &basis.context,
            &placement,
            &iso(expires_ms),
            &now,
        )?;
        if let Some(rule) = &placement.rule {
            let decider = Decider {
                kind: "policy",
                id: rule,
                level: Some(placement.level.as_str()),
                reason: None,
                scorer_version: None,
            };
            write_audit(
                &transaction,
                &approval_id,
                &new.action_digest,
                &decider,
                placement.status.as_str(),
                &now,
            )?;
        }
        if let Some(judgement) = &placement.judgement {
            // An escalated scorer row is audited as `escalated`: it is not
            // pending, the request climbs to the lead.
            let decision = if placement.status.is_pending() {
                "escalated"
            } else {
                placement.status.as_str()
            };
            let decider = Decider {
                kind: "scorer",
                id: SCORER_ACTOR,
                level: Some(ApprovalLevel::Scorer.as_str()),
                reason: placement.reason,
                scorer_version: Some(judgement.version()),
            };
            write_audit(
                &transaction,
                &approval_id,
                &new.action_digest,
                &decider,
                decision,
                &now,
            )?;
        }
        // Pending or decided, the conversation sees the approval at once.
        let created_row = approval_by_id(&transaction, &approval_id)?;
        approval_settle::record_timeline_line(&transaction, &created_row)?;
        let change = if placement.status.is_pending() {
            "created"
        } else {
            "decided"
        };
        let frame = frame_of(&transaction, &approval_id, change)?;
        transaction.commit()?;
        self.publish(frame);
        Ok(Some(approval_id))
    }
}

fn seat_of(context: &ApprovalContext, new: &NewApproval) -> Option<String> {
    context.seat_id.clone().or_else(|| new.seat_id.clone())
}

/// The row of a new approval, placed where the policy and the routing put it.
fn insert_approval(
    transaction: &Transaction<'_>,
    approval_id: &str,
    new: &NewApproval,
    context: &ApprovalContext,
    placement: &Placement,
    deadline: &str,
    now: &str,
) -> Result<(), ChatError> {
    let paths = serde_json::to_string(&new.paths).map_err(ChatError::from)?;
    let payload = new.payload_redacted.as_ref().map(|value| value.to_string());
    let decider = placement.decider();
    let decided_at = decider.map(|_| now);
    let decider_kind = decider.map(|(kind, _)| kind);
    let decider_id = decider.map(|(_, id)| id);
    let risk_score = placement.judgement.as_ref().and_then(Judgement::risk_score);
    let scorer_version = placement.judgement.as_ref().map(Judgement::version);
    transaction.execute(
        "INSERT INTO approval
            (id, company_id, conversation_id, delegation_id, seat_id, agent_label, run_id,
             pane_id, kind, tool, summary, cwd, paths_json, risk, action_digest,
             payload_redacted, status, scope, expires_at, decided_at, decider_kind,
             decider_id, level, escalated_from, reason, level_expires_at, risk_score,
             scorer_version, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 'once', ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?28)",
        params![
            approval_id,
            company_id(transaction)?,
            context.conversation_id,
            context.delegation_id,
            seat_of(context, new),
            new.agent_label,
            new.run_id,
            new.pane_id,
            new.kind.as_str(),
            new.tool,
            new.summary,
            new.cwd,
            paths,
            placement.risk.as_str(),
            new.action_digest,
            payload,
            placement.status.as_str(),
            deadline,
            decided_at,
            decider_kind,
            decider_id,
            placement.level.as_str(),
            placement.escalated_from.map(ApprovalLevel::as_str),
            placement.reason,
            placement.level_expires_at,
            risk_score,
            scorer_version,
            now
        ],
    )?;
    Ok(())
}
