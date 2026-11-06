//! The team summary (F1): one plain line per member of the team. A template
//! writes every headline, so no message, subject or path can reach one. The
//! inputs hold no free text in their activity, and the names never enter a
//! headline either.

use serde::Serialize;

use crate::clock::parse_iso;
use crate::model_approval::ApprovalKind;
use crate::store::{HireLink, PendingApproval, PendingPlan};

const HOUR_MS: i64 = 3_600_000;

/// What a member needs, most urgent first: the order of the variants is the order of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    WaitingForYou,
    Blocked,
    Working,
    Done,
    Idle,
}

impl TeamStatus {
    /// Every status, in the order of the list.
    pub const ALL: [TeamStatus; 5] = [
        TeamStatus::WaitingForYou,
        TeamStatus::Blocked,
        TeamStatus::Working,
        TeamStatus::Done,
        TeamStatus::Idle,
    ];
}

/// What the host knows about one member right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    WaitingApproval(ApprovalKind),
    /// A plan of `steps` steps the seat proposed and the operator has not answered.
    WaitingPlan {
        steps: u32,
    },
    Working {
        minutes: u32,
    },
    /// No count: no source says how many files a delegation changed.
    Done,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberInput {
    pub agent_id: String,
    pub name: String,
    pub role_title: String,
    /// When the current status began, as `%Y-%m-%dT%H:%M:%fZ`.
    pub since: String,
    pub activity: Activity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TeamMemberSummary {
    pub agent_id: String,
    pub name: String,
    pub role_title: String,
    pub status: TeamStatus,
    pub headline: String,
    pub since: String,
}

/// What a hire's seat is doing at `now_ms`, and since when. The person comes
/// first: an approval or a plan waiting beats a block, a block beats work, work
/// beats a finished delegation. `blocked` is the pane's own status, read by the host.
pub fn activity_of(link: &HireLink, blocked: bool, now_ms: i64) -> (Activity, String) {
    if let Some(PendingApproval { kind, created_at }) = &link.approval {
        return (Activity::WaitingApproval(*kind), created_at.clone());
    }
    if let Some(PendingPlan { steps, created_at }) = &link.plan {
        return (Activity::WaitingPlan { steps: *steps }, created_at.clone());
    }
    if blocked {
        return (Activity::Blocked, link.hired_at.clone());
    }
    if let Some(started) = &link.working_since {
        let minutes = parse_iso(started).map_or(0, |start| (now_ms - start).max(0) / 60_000);
        let minutes = u32::try_from(minutes).unwrap_or(u32::MAX);
        return (Activity::Working { minutes }, started.clone());
    }
    match &link.finished_at {
        Some(finished) if parse_iso(finished).is_some_and(|end| now_ms - end < HOUR_MS) => {
            (Activity::Done, finished.clone())
        }
        _ => (Activity::Idle, link.hired_at.clone()),
    }
}

/// The members in the order the person reads them: by status, then the newest first.
pub fn summarize(inputs: &[MemberInput]) -> Vec<TeamMemberSummary> {
    let mut members: Vec<TeamMemberSummary> = inputs.iter().map(summary).collect();
    // Fixed-width UTC stamps sort as text, so the newest is the largest.
    members.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| b.since.cmp(&a.since)));
    members
}

fn summary(input: &MemberInput) -> TeamMemberSummary {
    let (status, headline) = status_and_headline(&input.activity);
    TeamMemberSummary {
        agent_id: input.agent_id.clone(),
        name: input.name.clone(),
        role_title: input.role_title.clone(),
        status,
        headline,
        since: input.since.clone(),
    }
}

/// The status a member shows for an activity: the one vocabulary of the team.
pub fn status_of(activity: &Activity) -> TeamStatus {
    match activity {
        Activity::Idle => TeamStatus::Idle,
        Activity::Blocked => TeamStatus::Blocked,
        Activity::WaitingApproval(_) | Activity::WaitingPlan { .. } => TeamStatus::WaitingForYou,
        Activity::Working { .. } => TeamStatus::Working,
        Activity::Done => TeamStatus::Done,
    }
}

fn status_and_headline(activity: &Activity) -> (TeamStatus, String) {
    let headline = match activity {
        Activity::Idle => "Sin actividad".to_string(),
        Activity::Blocked => "Necesita ayuda".to_string(),
        Activity::WaitingApproval(kind) => format!("Espera tu visto bueno para {}", action(*kind)),
        // The count only: the step titles are the plan's text and stay out.
        Activity::WaitingPlan { steps } => {
            format!("Espera tu visto bueno para un plan de {steps} pasos")
        }
        Activity::Working { minutes } => format!("Trabajando desde hace {minutes} min"),
        Activity::Done => "Terminó".to_string(),
    };
    (status_of(activity), headline)
}

fn action(kind: ApprovalKind) -> &'static str {
    match kind {
        ApprovalKind::Command => "ejecutar un comando",
        ApprovalKind::Network => "usar internet",
        ApprovalKind::Edit => "cambiar ficheros",
        ApprovalKind::Tool => "usar una herramienta",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, since: &str, activity: Activity) -> MemberInput {
        MemberInput {
            agent_id: format!("hire_{name}"),
            name: name.to_string(),
            role_title: "Programadora".to_string(),
            since: since.to_string(),
            activity,
        }
    }

    fn only(activity: Activity) -> TeamMemberSummary {
        summarize(&[member("Ana", "2026-11-05T10:00:00.000Z", activity)])
            .into_iter()
            .next()
            .expect("one member")
    }

    #[test]
    fn each_activity_has_its_status_and_template_headline() {
        let cases = [
            (Activity::Idle, TeamStatus::Idle, "Sin actividad"),
            (Activity::Blocked, TeamStatus::Blocked, "Necesita ayuda"),
            (
                Activity::Working { minutes: 12 },
                TeamStatus::Working,
                "Trabajando desde hace 12 min",
            ),
            (Activity::Done, TeamStatus::Done, "Terminó"),
            (
                Activity::WaitingApproval(ApprovalKind::Command),
                TeamStatus::WaitingForYou,
                "Espera tu visto bueno para ejecutar un comando",
            ),
        ];
        for (activity, status, headline) in cases {
            let summary = only(activity);
            assert_eq!(summary.status, status);
            assert_eq!(summary.headline, headline);
        }
    }

    #[test]
    fn each_approval_kind_names_its_action_and_nothing_else() {
        let expected = [
            (ApprovalKind::Command, "ejecutar un comando"),
            (ApprovalKind::Network, "usar internet"),
            (ApprovalKind::Edit, "cambiar ficheros"),
            (ApprovalKind::Tool, "usar una herramienta"),
        ];
        for (kind, action) in expected {
            let headline = only(Activity::WaitingApproval(kind)).headline;
            assert_eq!(headline, format!("Espera tu visto bueno para {action}"));
        }
    }

    #[test]
    fn the_person_comes_first_then_the_newest_within_each_group() {
        let inputs = [
            member("idle", "2026-11-05T09:00:00.000Z", Activity::Idle),
            member("done_old", "2026-11-05T08:00:00.000Z", Activity::Done),
            member(
                "work_old",
                "2026-11-05T07:00:00.000Z",
                Activity::Working { minutes: 5 },
            ),
            member("blocked", "2026-11-05T06:00:00.000Z", Activity::Blocked),
            member(
                "work_new",
                "2026-11-05T10:00:00.000Z",
                Activity::Working { minutes: 1 },
            ),
            member("done_new", "2026-11-05T11:00:00.000Z", Activity::Done),
            member(
                "waiting",
                "2026-11-05T05:00:00.000Z",
                Activity::WaitingApproval(ApprovalKind::Edit),
            ),
        ];
        let names: Vec<String> = summarize(&inputs).into_iter().map(|m| m.name).collect();
        assert_eq!(
            names,
            ["waiting", "blocked", "work_new", "work_old", "done_new", "done_old", "idle"]
        );
    }

    /// The inputs carry names and titles, never text from a message, a subject or a path:
    /// none of them may reach a headline, whatever the status.
    #[test]
    fn no_input_text_reaches_a_headline() {
        let secret = "Asunto: transferencia /home/max/banco.csv";
        let activities = [
            Activity::Idle,
            Activity::Blocked,
            Activity::Working { minutes: 3 },
            Activity::Done,
            Activity::WaitingApproval(ApprovalKind::Network),
        ];
        for activity in activities {
            let input = MemberInput {
                agent_id: secret.to_string(),
                name: secret.to_string(),
                role_title: secret.to_string(),
                since: "2026-11-05T10:00:00.000Z".to_string(),
                activity,
            };
            let summary = summarize(&[input]).remove(0);
            assert!(!summary.headline.contains("Asunto"), "{}", summary.headline);
            assert!(!summary.headline.contains("/home"), "{}", summary.headline);
            assert!(!summary.headline.contains("banco"), "{}", summary.headline);
        }
    }

    #[test]
    fn an_empty_team_summarizes_to_nothing() {
        assert!(summarize(&[]).is_empty());
    }

    const NOW: &str = "2026-11-05T12:00:00.000Z";

    fn facts() -> HireLink {
        HireLink {
            hire_id: "hire_1".to_string(),
            name: "Bob".to_string(),
            catalog_role: "programador".to_string(),
            hired_at: "2026-11-05T08:00:00.000Z".to_string(),
            seat_pane: None,
            approval: None,
            working_since: None,
            finished_at: None,
            plan: None,
        }
    }

    fn activity(link: &HireLink, blocked: bool) -> (Activity, String) {
        activity_of(link, blocked, parse_iso(NOW).expect("now"))
    }

    #[test]
    fn a_hire_with_no_activity_is_idle_since_it_was_hired() {
        assert_eq!(
            activity(&facts(), false),
            (Activity::Idle, "2026-11-05T08:00:00.000Z".to_string())
        );
    }

    #[test]
    fn a_running_delegation_is_working_for_the_minutes_since_it_started() {
        let mut link = facts();
        link.working_since = Some("2026-11-05T10:30:00.000Z".to_string());
        assert_eq!(
            activity(&link, false),
            (
                Activity::Working { minutes: 90 },
                "2026-11-05T10:30:00.000Z".to_string()
            )
        );
    }

    #[test]
    fn a_pending_approval_comes_before_blocked_and_working() {
        let mut link = facts();
        link.working_since = Some("2026-11-05T10:30:00.000Z".to_string());
        link.approval = Some(PendingApproval {
            kind: ApprovalKind::Edit,
            created_at: "2026-11-05T11:50:00.000Z".to_string(),
        });
        assert_eq!(
            activity(&link, true),
            (
                Activity::WaitingApproval(ApprovalKind::Edit),
                "2026-11-05T11:50:00.000Z".to_string()
            )
        );
    }

    #[test]
    fn a_blocked_pane_comes_before_working() {
        let mut link = facts();
        link.working_since = Some("2026-11-05T10:30:00.000Z".to_string());
        assert_eq!(activity(&link, true).0, Activity::Blocked);
    }

    #[test]
    fn a_delegation_finished_within_the_hour_is_done() {
        let mut link = facts();
        link.finished_at = Some("2026-11-05T11:30:00.000Z".to_string());
        assert_eq!(
            activity(&link, false),
            (Activity::Done, "2026-11-05T11:30:00.000Z".to_string())
        );
    }

    #[test]
    fn a_delegation_finished_over_an_hour_ago_is_idle_again() {
        let mut link = facts();
        link.finished_at = Some("2026-11-05T09:00:00.000Z".to_string());
        assert_eq!(activity(&link, false).0, Activity::Idle);
    }

    #[test]
    fn work_in_progress_outranks_a_finished_delegation() {
        let mut link = facts();
        link.working_since = Some("2026-11-05T11:55:00.000Z".to_string());
        link.finished_at = Some("2026-11-05T11:30:00.000Z".to_string());
        assert_eq!(activity(&link, false).0, Activity::Working { minutes: 5 });
    }

    #[test]
    fn a_proposed_plan_waits_for_you_since_it_was_proposed() {
        let mut link = facts();
        link.plan = Some(PendingPlan {
            steps: 4,
            created_at: "2026-11-05T11:40:00.000Z".to_string(),
        });
        assert_eq!(
            activity(&link, false),
            (
                Activity::WaitingPlan { steps: 4 },
                "2026-11-05T11:40:00.000Z".to_string()
            )
        );
    }

    #[test]
    fn a_plan_waits_after_an_approval_and_before_a_block() {
        let mut link = facts();
        link.plan = Some(PendingPlan {
            steps: 4,
            created_at: "2026-11-05T11:40:00.000Z".to_string(),
        });
        assert_eq!(activity(&link, true).0, Activity::WaitingPlan { steps: 4 });
        link.approval = Some(PendingApproval {
            kind: ApprovalKind::Edit,
            created_at: "2026-11-05T11:50:00.000Z".to_string(),
        });
        assert_eq!(
            activity(&link, true).0,
            Activity::WaitingApproval(ApprovalKind::Edit)
        );
    }

    #[test]
    fn a_plan_headline_names_only_the_count_of_steps() {
        let summary = only(Activity::WaitingPlan { steps: 4 });
        assert_eq!(summary.status, TeamStatus::WaitingForYou);
        assert_eq!(
            summary.headline,
            "Espera tu visto bueno para un plan de 4 pasos"
        );
    }
}
