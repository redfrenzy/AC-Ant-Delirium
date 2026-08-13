use super::*;

#[derive(Clone, Copy)]
struct StaticAuthoritySource {
    effective_uid: Option<u32>,
    service_uid: Option<u32>,
}

impl ServiceAuthoritySource for StaticAuthoritySource {
    fn effective_uid(&self) -> Option<u32> {
        self.effective_uid
    }

    fn account_uid(&self, account_name: &str) -> Option<u32> {
        assert_eq!(account_name, SERVICE_ACCOUNT_NAME);
        self.service_uid
    }
}

#[test]
fn dedicated_non_root_uid_creates_the_internal_authority_token() {
    let authority = verify_service_authority(&StaticAuthoritySource {
        effective_uid: Some(987),
        service_uid: Some(987),
    });

    assert_eq!(
        authority,
        Ok(VerifiedServiceAuthority { effective_uid: 987 })
    );
}

#[test]
fn argv_environment_and_same_user_execution_cannot_create_service_authority() {
    for source in [
        StaticAuthoritySource {
            effective_uid: Some(1000),
            service_uid: Some(987),
        },
        StaticAuthoritySource {
            effective_uid: Some(0),
            service_uid: Some(0),
        },
        StaticAuthoritySource {
            effective_uid: None,
            service_uid: Some(987),
        },
        StaticAuthoritySource {
            effective_uid: Some(987),
            service_uid: None,
        },
    ] {
        assert_eq!(
            verify_service_authority(&source),
            Err(PublicError::Authorization)
        );
    }
}

#[test]
fn linux_status_parser_uses_effective_uid_and_fails_closed_on_ambiguity() {
    assert_eq!(
        parse_effective_uid("Name:\tantdelirium\nUid:\t1000\t987\t1000\t1000\n"),
        Some(987)
    );
    for malformed in [
        "Uid:\t1000\n",
        "Uid:\t1000\tnot-a-uid\t1000\t1000\n",
        "Uid:\t1000\t987\t1000\t1000\nUid:\t1000\t987\t1000\t1000\n",
        "Name:\tantdelirium\n",
    ] {
        assert_eq!(parse_effective_uid(malformed), None);
    }
}

#[test]
fn system_unit_uses_the_same_binary_under_a_dedicated_system_identity() {
    let unit = include_str!("../../packaging/systemd/antdelirium.service.in");

    for required in [
        "ExecStart=@LIBEXECDIR@/antdelirium service",
        "User=antdelirium",
        "Group=antdelirium",
        "NoNewPrivileges=yes",
        "PrivateNetwork=yes",
        "ProtectSystem=strict",
        "ProtectHome=yes",
        "ProtectProc=invisible",
        "CapabilityBoundingSet=",
        "AmbientCapabilities=",
        "StateDirectory=antdelirium",
        "StateDirectoryMode=0700",
    ] {
        assert!(unit.lines().any(|line| line == required));
    }
    for forbidden in [
        "systemctl --user",
        "User=dracker",
        "$HOME",
        "/home/",
        "plugins/",
        "plan/",
    ] {
        assert!(!unit.contains(forbidden));
    }
}
