use super::*;

#[cfg(unix)]
use std::os::fd::{AsRawFd, IntoRawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

fn activation(
    pid: &str,
    count: &str,
    names: &str,
) -> Result<[ActivatedDescriptorSpec; 2], ServiceChannelError> {
    parse_activation_environment(
        4242,
        Some(OsStr::new(pid)),
        Some(OsStr::new(count)),
        Some(OsStr::new(names)),
    )
}

#[test]
fn activation_requires_exactly_two_named_descriptors_for_the_current_process() {
    assert_eq!(
        activation("4242", "2", "agent:admin"),
        Ok([
            ActivatedDescriptorSpec {
                kind: ServiceChannelKind::Limited,
                raw_fd: 3,
            },
            ActivatedDescriptorSpec {
                kind: ServiceChannelKind::Administrative,
                raw_fd: 4,
            },
        ])
    );
    assert_eq!(
        activation("4242", "2", "admin:agent"),
        Ok([
            ActivatedDescriptorSpec {
                kind: ServiceChannelKind::Administrative,
                raw_fd: 3,
            },
            ActivatedDescriptorSpec {
                kind: ServiceChannelKind::Limited,
                raw_fd: 4,
            },
        ])
    );
}

#[test]
fn activation_fails_closed_for_missing_extra_duplicate_or_forged_metadata() {
    for result in [
        activation("4241", "2", "agent:admin"),
        activation("+4242", "2", "agent:admin"),
        activation("4242", "1", "agent"),
        activation("4242", "3", "agent:admin:extra"),
        activation("4242", "2", "agent:agent"),
        activation("4242", "2", "agent:operator"),
        activation("4242", "2", "agent:"),
    ] {
        assert_eq!(result, Err(ServiceChannelError::ActivationContract));
    }
    assert_eq!(
        parse_activation_environment(4242, None, None, None),
        Err(ServiceChannelError::ActivationContract)
    );
}

#[test]
fn each_listener_has_a_distinct_non_multiplexed_magic_parser() {
    let limited = LimitedChannelParser::new();
    let administrative = AdministrativeChannelParser::new();

    assert!(limited.accepts_magic(*b"ADL1"));
    assert!(!limited.accepts_magic(*b"ADA1"));
    assert!(!limited.accepts_magic(*b"ADL2"));
    assert!(administrative.accepts_magic(*b"ADA1"));
    assert!(!administrative.accepts_magic(*b"ADL1"));
    assert!(!administrative.accepts_magic(*b"ADA2"));
}

#[cfg(unix)]
#[test]
fn listener_contract_requires_unix_stream_and_accept_state_together() {
    assert_eq!(
        validate_listener_facts(ListenerFacts {
            socket_type: libc::SOCK_STREAM,
            accepting: 1,
            address_family: libc::AF_UNIX,
            has_filesystem_address: true,
        }),
        Ok(())
    );
    for facts in [
        ListenerFacts {
            socket_type: libc::SOCK_DGRAM,
            accepting: 1,
            address_family: libc::AF_UNIX,
            has_filesystem_address: true,
        },
        ListenerFacts {
            socket_type: libc::SOCK_STREAM,
            accepting: 0,
            address_family: libc::AF_UNIX,
            has_filesystem_address: true,
        },
        ListenerFacts {
            socket_type: libc::SOCK_STREAM,
            accepting: 1,
            address_family: libc::AF_INET,
            has_filesystem_address: true,
        },
        ListenerFacts {
            socket_type: libc::SOCK_STREAM,
            accepting: 1,
            address_family: libc::AF_UNIX,
            has_filesystem_address: false,
        },
    ] {
        assert_eq!(
            validate_listener_facts(facts),
            Err(ServiceChannelError::DescriptorContract)
        );
    }
}

#[cfg(unix)]
#[test]
fn limited_and_administrative_channels_cannot_alias_the_same_socket() {
    let limited = ListenerIdentity {
        device: 7,
        inode: 11,
    };
    let administrative = ListenerIdentity {
        device: 7,
        inode: 12,
    };
    assert_eq!(
        require_distinct_listener_identities(&[limited, administrative]),
        Ok(())
    );
    assert_eq!(
        require_distinct_listener_identities(&[limited, limited]),
        Err(ServiceChannelError::DescriptorContract)
    );
    assert_eq!(
        require_distinct_listener_identities(&[limited]),
        Err(ServiceChannelError::DescriptorContract)
    );
}

#[cfg(unix)]
#[test]
fn connected_unix_streams_are_not_accepted_as_listeners() {
    let (first, second) = UnixStream::pair().expect("Unix stream fixture");
    let descriptors = [
        ActivatedDescriptorSpec {
            kind: ServiceChannelKind::Limited,
            raw_fd: first.into_raw_fd(),
        },
        ActivatedDescriptorSpec {
            kind: ServiceChannelKind::Administrative,
            raw_fd: second.into_raw_fd(),
        },
    ];

    assert!(matches!(
        ActivatedServiceChannels::claim(descriptors),
        Err(ServiceChannelError::DescriptorContract)
    ));
}

#[cfg(unix)]
#[test]
fn inherited_descriptors_are_marked_close_on_exec() {
    let (first, _second) = UnixStream::pair().expect("Unix stream fixture");
    let owned = unsafe { OwnedFd::from_raw_fd(first.into_raw_fd()) };

    set_close_on_exec(&owned).expect("set close-on-exec");
    let flags = unsafe { libc::fcntl(owned.as_raw_fd(), libc::F_GETFD) };
    assert!(flags >= 0);
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
}

#[test]
fn packaging_keeps_agent_and_administrative_socket_permissions_separate() {
    let service = include_str!("../../packaging/systemd/antdelirium.service.in");
    let limited = include_str!("../../packaging/systemd/antdelirium-agent.socket.in");
    let administrative = include_str!("../../packaging/systemd/antdelirium-admin.socket.in");

    assert!(service.contains("Requires=antdelirium-agent.socket antdelirium-admin.socket"));
    assert!(limited.contains("FileDescriptorName=agent"));
    assert!(limited.contains("Service=antdelirium.service"));
    assert!(limited.contains("SocketGroup=antdelirium-client"));
    assert!(limited.contains("DirectoryMode=0755"));
    assert!(limited.contains("Accept=no"));
    assert!(administrative.contains("FileDescriptorName=admin"));
    assert!(administrative.contains("Service=antdelirium.service"));
    assert!(administrative.contains("SocketUser=root"));
    assert!(administrative.contains("SocketGroup=antdelirium-admin"));
    assert!(administrative.contains("DirectoryMode=0750"));
    assert!(administrative.contains("Accept=no"));
    assert!(!limited.contains("antdelirium-admin"));
    assert!(!administrative.contains("antdelirium-client"));
}
