//! `style.learn_sent` (RFC estilo-aprendido §2.1, F1) over a scripted Microsoft: twenty
//! sent mails are counted on the host, quoted text is left out, the proposals carry no
//! name, address or link, a limit outside 1..=100 is refused, and `estilo.md` never changes.

mod mail_support;

use std::path::Path;
use std::sync::Arc;

use excubia_chat::{ChatStore, ToolRequest};
use excubia_host::mail_send::{MailSends, SendPolicy, DEFAULT_WINDOW};
use excubia_host::mail_tools::{open_ledger, MailTools};
use excubia_mail::auth::Tenant;
use excubia_mail::{Account, AccountStore, FileKey, HttpTransport, Secret, Tokens};
use excubia_seat::{parse_tool, StyleLearnQuery};
use excubia_test_dir::TestDir;
use mail_support::{ok, token_answer, Scripted, ACCESS};
use serde_json::{json, Value};

const OLD_REFRESH: &str = "REFRESCO-ANTIGUO";
const NAMES: [&str; 5] = ["Juan", "María", "Ana", "Max", "Ruiz"];

fn tools(
    dir: &TestDir,
    transport: &Arc<Scripted>,
    chat: &Arc<ChatStore>,
) -> (MailTools, Arc<AccountStore>) {
    let data = dir.join("data");
    let key = FileKey::new(dir.join("key"));
    let store = Arc::new(AccountStore::open(data.join("mail/accounts"), &key).expect("store"));
    let transport: Arc<dyn HttpTransport> = transport.clone();
    let client_id = Some("app".to_owned());
    let sends = MailSends::new(
        Arc::clone(&transport),
        Arc::clone(&store),
        client_id.clone(),
        open_ledger(&data).expect("ledger"),
        &data.join("mail/accounts"),
        Arc::clone(chat),
        SendPolicy {
            per_hour: 10,
            window: DEFAULT_WINDOW,
        },
    );
    let tools = MailTools::new(
        transport,
        Arc::clone(&store),
        client_id,
        &data,
        Arc::clone(chat),
        &dir.join("workspaces"),
        sends,
    )
    .expect("mail tools");
    (tools, store)
}

fn sign_in(store: &AccountStore, label: &str, refresh: &str) {
    let tokens = Tokens::new(ACCESS, Some(Secret::new(refresh)), 3600);
    let account = Account::new(label, Tenant::Organizations, "ana@example.com", tokens);
    store.save(&account).expect("save");
}

/// Every file under `dir` read as text and joined: the audit line is somewhere in there.
fn written(dir: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            text.push_str(&written(&path));
        } else if let Ok(body) = std::fs::read_to_string(&path) {
            text.push_str(&body);
        }
    }
    text
}

const FORMAL: &str = "Cordial saludo, Don Juan\n\nLe comparto el informe; revísalo antes del viernes.\n\nCordialmente\nAna Ruiz\n";
const FRIENDLY: &str = "Hola María,\n\nGracias por todo. Nos vemos mañana.\n\nGracias\nMax\n";
const REPLY: &str = "Hola,\n\nSí, de acuerdo.\n\nEl lunes, 3 de noviembre de 2026, Juan escribió:\n> Dijiste algo; raro\n";

/// Twenty sent mails: twelve formal with a `;` each, seven friendly, one reply whose only
/// `;` sits in the quoted text.
fn twenty_sent_mails() -> Vec<String> {
    let mut bodies = vec![FORMAL.to_owned(); 12];
    bodies.extend(std::iter::repeat_n(FRIENDLY.to_owned(), 7));
    bodies.push(REPLY.to_owned());
    bodies
}

fn listing(count: usize) -> Value {
    let items: Vec<Value> = (0..count)
        .map(|i| {
            json!({
                "id": format!("s{i}"),
                "subject": "Asunto de prueba",
                "from": {"emailAddress": {"name": "Max", "address": "max@x.com"}},
                "receivedDateTime": "2026-11-10T08:00:00Z",
                "isRead": true,
                "hasAttachments": false,
                "bodyPreview": "",
            })
        })
        .collect();
    json!({ "value": items })
}

fn message(id: usize, body: &str) -> Value {
    json!({
        "id": format!("s{id}"),
        "subject": "Asunto de prueba",
        "from": {"emailAddress": {"name": "Max", "address": "max@x.com"}},
        "sender": {"emailAddress": {"name": "Max", "address": "max@x.com"}},
        "toRecipients": [{"emailAddress": {"name": "Juan", "address": "juan@x.com"}}],
        "receivedDateTime": "2026-11-10T08:00:00Z",
        "internetMessageId": "<s@x.com>",
        "hasAttachments": false,
        "body": {"contentType": "text", "content": body},
        "internetMessageHeaders": [],
    })
}

#[tokio::test]
async fn twenty_sent_mails_are_counted_on_the_host_and_quoted_text_does_not_count() {
    let dir = TestDir::new("style-learn-count");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let bodies = twenty_sent_mails();
    let mut answers = vec![token_answer(), ok(listing(bodies.len()))];
    for (i, body) in bodies.iter().enumerate() {
        answers.push(ok(message(i, body)));
        answers.push(ok(json!({"value": []})));
    }
    let transport = Arc::new(Scripted::new(answers));
    let (tools, store) = tools(&dir, &transport, &chat);
    sign_in(&store, "trabajo", OLD_REFRESH);

    let reply = tools
        .style_learn(&StyleLearnQuery {
            account: "trabajo".into(),
            limit: 20,
        })
        .await
        .expect("learn");

    let result = &reply.result;
    assert_eq!(result["ok"], true);
    assert_eq!(result["analyzed"], 20);
    // The `;` inside the quoted reply is not the operator's: 12 mails, 12 marks.
    assert_eq!(result["stats"]["semicolons"]["mails"], 12);
    assert_eq!(result["stats"]["semicolons"]["uses"], 12);
    assert_eq!(result["stats"]["greeting"]["text"], "Cordial saludo");
    assert_eq!(result["stats"]["greeting"]["mails"], 12);
    assert_eq!(result["stats"]["closing"]["text"], "Cordialmente");
    assert_eq!(result["stats"]["closing"]["mails"], 12);
    assert_eq!(result["stats"]["treatment"]["usted"], 12);
    assert_eq!(result["stats"]["treatment"]["tu"], 0);

    let proposals: Vec<&str> = result["proposals"]
        .as_array()
        .expect("list")
        .iter()
        .map(|p| p.as_str().expect("text"))
        .collect();
    assert!((3..=8).contains(&proposals.len()), "{proposals:?}");
    // 12 of 20 is a habit, so no rule against it is proposed.
    assert!(
        !proposals.iter().any(|p| p.contains("punto y coma")),
        "{proposals:?}"
    );
    assert!(proposals.contains(&"Saluda con «Cordial saludo»."));
    assert!(proposals.contains(&"Despídete con «Cordialmente»."));
    assert!(proposals.contains(&"Trata de usted."));

    for proposal in &proposals {
        assert!(!proposal.contains('@'), "{proposal}");
        assert!(!proposal.contains("http"), "{proposal}");
        let digits = proposal.chars().filter(|c| c.is_ascii_digit()).count();
        assert!(digits < 7, "{proposal}");
        for name in NAMES {
            assert!(!proposal.contains(name), "{proposal} carries {name}");
        }
    }

    let asked = transport.asked();
    assert_eq!(
        asked.len(),
        2 + 2 * 20,
        "the refresh, the listing, two reads per mail"
    );
    assert!(
        asked[1].contains("/me/mailFolders/sentitems/messages?$top=20"),
        "{}",
        asked[1]
    );
    let audit = written(&dir.join("data"));
    assert!(audit.contains("style.learn_sent"), "the read is audited");
    assert!(
        !audit.contains("Asunto de prueba"),
        "no subject in the audit"
    );
}

#[tokio::test]
async fn learning_never_changes_the_style_file() {
    let dir = TestDir::new("style-learn-readonly");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let bodies = [FORMAL.to_owned(), FRIENDLY.to_owned()];
    let mut answers = vec![token_answer(), ok(listing(bodies.len()))];
    for (i, body) in bodies.iter().enumerate() {
        answers.push(ok(message(i, body)));
        answers.push(ok(json!({"value": []})));
    }
    let transport = Arc::new(Scripted::new(answers));
    let (tools, store) = tools(&dir, &transport, &chat);
    sign_in(&store, "trabajo", OLD_REFRESH);
    let profile = dir.join("data/estilo/estilo.md");
    let before = std::fs::read(&profile).expect("seeded profile");

    tools
        .style_learn(&StyleLearnQuery {
            account: "trabajo".into(),
            limit: 2,
        })
        .await
        .expect("learn");

    assert_eq!(std::fs::read(&profile).expect("still there"), before);
}

#[test]
fn a_limit_outside_1_to_100_is_refused_as_an_argument_error() {
    for limit in [0, 101] {
        let refused = parse_tool(&ToolRequest {
            tool: "style.learn_sent".into(),
            arguments: json!({ "account": "trabajo", "limit": limit }),
        });
        assert_eq!(
            refused.expect_err("out of range").code,
            "invalid_arguments",
            "limit {limit}"
        );
    }
}
