//! The verdict rules of `mail.triage` (F0), one row per mail: the level and the signals
//! each set of checks gets, the signals always in the order of the enum.

use excubia_core_mail::received::{AuthSummary, Checks};
use excubia_core_mail::triage::{verdict, Level::*, Signal::*, Verdict};

const PASS: [&str; 3] = ["pass", "pass", "pass"];
const FAIL: [&str; 3] = ["fail", "fail", "fail"];
const MISSING: [&str; 3] = ["missing", "missing", "missing"];

/// A mail with the given SPF, DKIM and DMARC results and no other flag set; `edit`
/// sets the flags the case needs.
fn mail(auth: [&'static str; 3], edit: fn(&mut Checks)) -> Checks {
    let mut checks = Checks {
        from_domain: Some("banco.es".into()),
        reply_to_domain: None,
        return_path_domain: None,
        reply_to_differs: false,
        return_path_differs: false,
        auth: AuthSummary {
            spf: auth[0],
            dkim: auth[1],
            dmarc: auth[2],
        },
        claims_attachment_but_none: false,
        link_host_differs: false,
        list_unsubscribe: false,
        one_click_unsubscribe: false,
        bulk: false,
    };
    edit(&mut checks);
    checks
}

fn nothing(_: &mut Checks) {}

#[test]
fn every_row_gets_the_level_and_the_signals_its_rules_give() {
    let rows: Vec<(&str, Checks, Verdict)> = vec![
        (
            "phish clásico: auth fallida y Reply-To ajeno",
            mail(FAIL, |c| c.reply_to_differs = true),
            Verdict {
                level: PhishCandidate,
                signals: vec![AuthAllFail, DmarcFail, SpfFail, DkimFail, ReplyToDiffers],
            },
        ),
        (
            "softfail cuenta como fallo en los tres",
            mail(["softfail", "softfail", "fail"], nothing),
            Verdict {
                level: PhishCandidate,
                signals: vec![AuthAllFail, DmarcFail, SpfFail, DkimFail],
            },
        ),
        (
            "DMARC fallido y Return-Path ajeno con SPF/DKIM bien",
            mail(["pass", "pass", "fail"], |c| c.return_path_differs = true),
            Verdict {
                level: PhishCandidate,
                signals: vec![DmarcFail, ReturnPathDiffers],
            },
        ),
        (
            "DMARC fallido y enlace que muestra otro host",
            mail(["pass", "pass", "fail"], |c| c.link_host_differs = true),
            Verdict {
                level: PhishCandidate,
                signals: vec![DmarcFail, LinkHostDiffers],
            },
        ),
        (
            "DMARC fallido y adjunto anunciado que no está",
            mail(["pass", "pass", "fail"], |c| {
                c.claims_attachment_but_none = true
            }),
            Verdict {
                level: PhishCandidate,
                signals: vec![DmarcFail, ClaimsAttachmentButNone],
            },
        ),
        (
            "DMARC fallido solo: sospechoso, no candidato",
            mail(["pass", "pass", "fail"], nothing),
            Verdict {
                level: Suspicious,
                signals: vec![DmarcFail],
            },
        ),
        (
            "SPF fallido solo",
            mail(["fail", "pass", "pass"], nothing),
            Verdict {
                level: Suspicious,
                signals: vec![SpfFail],
            },
        ),
        (
            "DKIM en softfail solo",
            mail(["pass", "softfail", "pass"], nothing),
            Verdict {
                level: Suspicious,
                signals: vec![DkimFail],
            },
        ),
        (
            "SPF y DKIM fallidos con DMARC bien",
            mail(["fail", "fail", "pass"], nothing),
            Verdict {
                level: Suspicious,
                signals: vec![SpfFail, DkimFail],
            },
        ),
        (
            "adjunto anunciado que no está, auth correcta",
            mail(PASS, |c| c.claims_attachment_but_none = true),
            Verdict {
                level: Suspicious,
                signals: vec![ClaimsAttachmentButNone],
            },
        ),
        (
            "enlace que muestra otro host, auth correcta",
            mail(PASS, |c| c.link_host_differs = true),
            Verdict {
                level: Suspicious,
                signals: vec![LinkHostDiffers],
            },
        ),
        (
            "correo personal con Reply-To ajeno",
            mail(PASS, |c| c.reply_to_differs = true),
            Verdict {
                level: Suspicious,
                signals: vec![ReplyToDiffers],
            },
        ),
        (
            "correo personal con Return-Path ajeno",
            mail(PASS, |c| c.return_path_differs = true),
            Verdict {
                level: Suspicious,
                signals: vec![ReturnPathDiffers],
            },
        ),
        (
            "boletín legítimo: Return-Path y Reply-To del proveedor de envío",
            mail(PASS, |c| {
                c.bulk = true;
                c.one_click_unsubscribe = true;
                c.return_path_differs = true;
                c.reply_to_differs = true;
            }),
            Verdict {
                level: Bulk,
                signals: vec![
                    ReplyToDiffers,
                    ReturnPathDiffers,
                    BulkList,
                    OneClickUnsubscribe,
                ],
            },
        ),
        (
            "boletín con auth correcta y enlace que muestra otro host: Bulk",
            mail(PASS, |c| {
                c.bulk = true;
                c.link_host_differs = true;
            }),
            Verdict {
                level: Bulk,
                signals: vec![LinkHostDiffers, BulkList],
            },
        ),
        (
            "boletín con auth correcta y adjunto anunciado que no está: Suspicious",
            mail(PASS, |c| {
                c.bulk = true;
                c.claims_attachment_but_none = true;
            }),
            Verdict {
                level: Suspicious,
                signals: vec![ClaimsAttachmentButNone, BulkList],
            },
        ),
        (
            "boletín con DKIM neutral y enlace que muestra otro host: Suspicious",
            mail(["pass", "neutral", "pass"], |c| {
                c.bulk = true;
                c.link_host_differs = true;
            }),
            Verdict {
                level: Suspicious,
                signals: vec![LinkHostDiffers, BulkList],
            },
        ),
        (
            "boletín con DKIM neutral y Return-Path ajeno: Suspicious",
            mail(["pass", "neutral", "pass"], |c| {
                c.bulk = true;
                c.return_path_differs = true;
            }),
            Verdict {
                level: Suspicious,
                signals: vec![ReturnPathDiffers, BulkList],
            },
        ),
        (
            "lista en masa sin nada raro",
            mail(PASS, |c| c.bulk = true),
            Verdict {
                level: Bulk,
                signals: vec![BulkList],
            },
        ),
        (
            "lista en masa con SPF fallido: no es Bulk",
            mail(["fail", "pass", "pass"], |c| c.bulk = true),
            Verdict {
                level: Suspicious,
                signals: vec![SpfFail, BulkList],
            },
        ),
        (
            "lista en masa con DMARC fallido y Return-Path ajeno: candidato",
            mail(["pass", "pass", "fail"], |c| {
                c.bulk = true;
                c.return_path_differs = true;
            }),
            Verdict {
                level: PhishCandidate,
                signals: vec![DmarcFail, ReturnPathDiffers, BulkList],
            },
        ),
        (
            "correo personal limpio",
            mail(PASS, nothing),
            Verdict {
                level: Clean,
                signals: vec![],
            },
        ),
        (
            "DMARC sin política (none) y resto correcto: limpio",
            mail(["pass", "pass", "none"], nothing),
            Verdict {
                level: Clean,
                signals: vec![],
            },
        ),
        (
            "neutral en SPF no cuenta como fallo",
            mail(["neutral", "pass", "pass"], nothing),
            Verdict {
                level: Clean,
                signals: vec![],
            },
        ),
        (
            "solo cabecera de baja de un ESP: limpio, la señal queda",
            mail(PASS, |c| c.one_click_unsubscribe = true),
            Verdict {
                level: Clean,
                signals: vec![OneClickUnsubscribe],
            },
        ),
        (
            "sin cabeceras de autenticación: no se puede comprobar",
            mail(MISSING, nothing),
            Verdict {
                level: Unknown,
                signals: vec![AuthMissing],
            },
        ),
        (
            "missing y none mezclados: no se puede comprobar",
            mail(["missing", "none", "none"], nothing),
            Verdict {
                level: Unknown,
                signals: vec![AuthMissing],
            },
        ),
        (
            "sin autenticación y lista en masa: boletín, no desconocido",
            mail(MISSING, |c| c.bulk = true),
            Verdict {
                level: Bulk,
                signals: vec![AuthMissing, BulkList],
            },
        ),
        (
            "sin autenticación y Reply-To ajeno: sospechoso",
            mail(MISSING, |c| c.reply_to_differs = true),
            Verdict {
                level: Suspicious,
                signals: vec![AuthMissing, ReplyToDiffers],
            },
        ),
        (
            "sin autenticación y adjunto anunciado que no está: sospechoso",
            mail(MISSING, |c| c.claims_attachment_but_none = true),
            Verdict {
                level: Suspicious,
                signals: vec![AuthMissing, ClaimsAttachmentButNone],
            },
        ),
        (
            "todas las señales a la vez: el orden es el del enum",
            mail(FAIL, |c| {
                c.reply_to_differs = true;
                c.return_path_differs = true;
                c.claims_attachment_but_none = true;
                c.link_host_differs = true;
                c.bulk = true;
                c.one_click_unsubscribe = true;
            }),
            Verdict {
                level: PhishCandidate,
                signals: vec![
                    AuthAllFail,
                    DmarcFail,
                    SpfFail,
                    DkimFail,
                    ReplyToDiffers,
                    ReturnPathDiffers,
                    ClaimsAttachmentButNone,
                    LinkHostDiffers,
                    BulkList,
                    OneClickUnsubscribe,
                ],
            },
        ),
    ];
    assert!(rows.len() >= 20, "{} rows", rows.len());
    for (name, checks, expected) in rows {
        assert_eq!(verdict(&checks), expected, "{name}");
    }
}
