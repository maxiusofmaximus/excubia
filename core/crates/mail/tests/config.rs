mod support;

use excubia_mail::{MailConfig, DEFAULT_MAX_ATTACHMENT_BYTES, HARD_MAX_ATTACHMENT_BYTES};

#[test]
fn the_toml_section_supplies_the_client_and_the_limits() {
    let config = MailConfig::from_toml(
        r#"
[mail]
client_id = "abc-123"
accounts_dir = "cuentas"
max_attachment_bytes = 1048576
"#,
        None,
    )
    .expect("config");
    assert_eq!(config.client_id, "abc-123");
    assert_eq!(config.accounts_dir.to_string_lossy(), "cuentas");
    assert_eq!(config.max_attachment_bytes, 1_048_576);
}

#[test]
fn the_environment_supplies_the_client_when_the_file_does_not() {
    let config = MailConfig::from_toml("[mail]\n", Some("env-client")).expect("config");
    assert_eq!(config.client_id, "env-client");
    assert_eq!(config.max_attachment_bytes, DEFAULT_MAX_ATTACHMENT_BYTES);
    assert!(!config.accounts_dir.as_os_str().is_empty());
}

#[test]
fn the_file_wins_over_the_environment() {
    let config = MailConfig::from_toml("[mail]\nclient_id = \"file-client\"\n", Some("env-client"))
        .expect("config");
    assert_eq!(config.client_id, "file-client");
}

#[test]
fn a_clean_configuration_selects_the_public_client() {
    let config = MailConfig::from_toml("[mail]\n", None).expect("config");
    assert_eq!(config.client_id, excubia_mail::PUBLIC_CLIENT_ID);
    assert_eq!(config.client_id, "fed6c50b-ecd4-43c4-8ff1-7162afda228a");
    // A blank value is not an override: the public client still applies.
    let blank = MailConfig::from_toml("[mail]\nclient_id = \"  \"\n", Some("   ")).expect("config");
    assert_eq!(blank.client_id, excubia_mail::PUBLIC_CLIENT_ID);
}

#[test]
fn an_explicit_environment_beats_the_public_client_and_the_file_beats_both() {
    let env = MailConfig::from_toml("[mail]\n", Some("operator-env")).expect("config");
    assert_eq!(env.client_id, "operator-env");
    let file = MailConfig::from_toml(
        "[mail]\nclient_id = \"operator-file\"\n",
        Some("operator-env"),
    )
    .expect("config");
    assert_eq!(file.client_id, "operator-file");
}

#[test]
fn the_attachment_ceiling_is_bounded() {
    let too_big = format!(
        "[mail]\nclient_id = \"x\"\nmax_attachment_bytes = {}\n",
        HARD_MAX_ATTACHMENT_BYTES + 1
    );
    let error = MailConfig::from_toml(&too_big, None).expect_err("too big");
    assert_eq!(error.code(), "mail_invalid_config");
    let zero = MailConfig::from_toml(
        "[mail]\nclient_id = \"x\"\nmax_attachment_bytes = 0\n",
        None,
    )
    .expect_err("zero");
    assert_eq!(zero.code(), "mail_invalid_config");
}

#[test]
fn a_malformed_file_is_an_invalid_configuration() {
    let error = MailConfig::from_toml("mail = [", None).expect_err("toml");
    assert_eq!(error.code(), "mail_invalid_config");
}
