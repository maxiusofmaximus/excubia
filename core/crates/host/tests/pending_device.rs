//! D3: a newly paired device is pending. It holds no permission, so every
//! route refuses it until the host grants one.

mod harness;

use harness::TestHost;
use hn_core_auth::Permission;

#[tokio::test(flavor = "multi_thread")]
async fn a_new_device_is_refused_until_the_host_grants_read_state() {
    let host = TestHost::start("pending-device").await;
    let (client, paired) = host.pair("Phone").await;
    assert!(paired.permissions.is_empty(), "{:?}", paired.permissions);

    let refused = client
        .state()
        .await
        .expect_err("a pending device reads nothing");
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert!(refused.body.contains("forbidden"), "{}", refused.body);

    host.grant(&paired.device_id, Permission::ReadState);
    assert!(client.state().await.is_ok());
}
