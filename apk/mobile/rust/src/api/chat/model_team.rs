//! Team models: projects from `/v1/chat/bootstrap` and the full delegation
//! record from `/v1/chat/delegations/{id}`. Cards themselves stay in
//! [`super::model`] because they share the timeline `seq` space.

use serde_json::Value;

use super::model::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSeat {
    pub agent_id: String,
    pub name: String,
    pub status: String,
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatProject {
    pub id: String,
    pub name: String,
    pub root_label: String,
    pub status: String,
    pub lead: Option<ChatSeat>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDelegationChild {
    pub id: String,
    pub depth: i64,
    pub state: String,
    pub to_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDelegation {
    pub id: String,
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub parent_id: Option<String>,
    pub from_agent_id: String,
    pub from_name: String,
    pub to_agent_id: String,
    pub to_name: String,
    pub task: String,
    pub state: String,
    pub depth: i64,
    pub pane_id: Option<String>,
    pub result_message_id: Option<String>,
    pub reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub result_body: Option<String>,
    pub children: Vec<ChatDelegationChild>,
    pub synthesis: Option<String>,
}

pub(crate) fn project_from(value: &Value) -> Option<ChatProject> {
    Some(ChatProject {
        id: text(value, "id")?,
        name: text(value, "name")?,
        root_label: text(value, "root_label").unwrap_or_default(),
        status: text(value, "status").unwrap_or_else(|| "active".to_string()),
        lead: value.get("lead").and_then(seat_from),
    })
}

pub(crate) fn seat_from(value: &Value) -> Option<ChatSeat> {
    Some(ChatSeat {
        agent_id: text(value, "agent_id")?,
        name: text(value, "name")?,
        status: text(value, "status").unwrap_or_else(|| "unknown".to_string()),
        pane_id: text(value, "pane_id"),
    })
}

pub(crate) fn delegation_from(value: &Value) -> Option<ChatDelegation> {
    let record = value.get("delegation")?;
    Some(ChatDelegation {
        id: text(record, "id")?,
        conversation_id: text(record, "conversation_id").unwrap_or_default(),
        project_id: text(record, "project_id"),
        parent_id: text(record, "parent_id"),
        from_agent_id: text(record, "from_agent_id").unwrap_or_default(),
        from_name: text(record, "from_name").unwrap_or_else(|| "Alfred".to_string()),
        to_agent_id: text(record, "to_agent_id").unwrap_or_default(),
        to_name: text(record, "to_name").unwrap_or_else(|| "el equipo".to_string()),
        task: text(record, "task").unwrap_or_default(),
        state: text(record, "state").unwrap_or_else(|| "queued".to_string()),
        depth: record.get("depth").and_then(Value::as_i64).unwrap_or(0),
        pane_id: text(record, "pane_id"),
        result_message_id: text(record, "result_message_id"),
        reason: text(record, "reason"),
        created_at: text(record, "created_at").unwrap_or_default(),
        updated_at: text(record, "updated_at").unwrap_or_default(),
        finished_at: text(record, "finished_at"),
        result_body: value
            .get("result_message")
            .and_then(|message| text(message, "body_md")),
        children: value
            .get("children")
            .and_then(Value::as_array)
            .map(|children| children.iter().filter_map(child_from).collect())
            .unwrap_or_default(),
        synthesis: value
            .get("synthesis")
            .and_then(|synthesis| text(synthesis, "state")),
    })
}

fn child_from(value: &Value) -> Option<ChatDelegationChild> {
    Some(ChatDelegationChild {
        id: text(value, "id")?,
        depth: value.get("depth").and_then(Value::as_i64).unwrap_or(0),
        state: text(value, "state").unwrap_or_else(|| "queued".to_string()),
        to_name: text(value, "to_name"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_carry_the_lead_and_hide_the_root_path() {
        let value: Value = serde_json::from_str(
            r#"{"id":"prj_1","name":"Demo","root_label":"demo","status":"active",
                "lead":{"agent_id":"ag_1","name":"Ernesto","status":"working","pane_id":"w1:p3"}}"#,
        )
        .unwrap();
        let project = project_from(&value).unwrap();
        assert_eq!(project.name, "Demo");
        assert_eq!(project.root_label, "demo");
        assert_eq!(project.lead.unwrap().pane_id.as_deref(), Some("w1:p3"));
    }

    #[test]
    fn a_delegation_detail_carries_the_result_children_and_synthesis() {
        let value: Value = serde_json::from_str(
            r#"{"ok":true,
                "delegation":{"id":"dlg_1","conversation_id":"cnv_1","from_name":"Alfred",
                  "to_name":"Ernesto","task":"Planifica","state":"done","depth":1,
                  "pane_id":"w1:p3","created_at":"a","updated_at":"b"},
                "children":[{"id":"dlg_2","depth":2,"state":"done","to_name":"Bob"}],
                "result_message":{"id":"msg_1","body_md":"Plan listo"},
                "synthesis":{"state":"queued"}}"#,
        )
        .unwrap();
        let detail = delegation_from(&value).unwrap();
        assert_eq!(detail.to_name, "Ernesto");
        assert_eq!(detail.state, "done");
        assert_eq!(detail.result_body.as_deref(), Some("Plan listo"));
        assert_eq!(detail.children[0].to_name.as_deref(), Some("Bob"));
        assert_eq!(detail.synthesis.as_deref(), Some("queued"));
    }

    #[test]
    fn a_delegation_without_a_record_is_not_decoded() {
        assert!(delegation_from(&serde_json::json!({"ok":true})).is_none());
    }
}
