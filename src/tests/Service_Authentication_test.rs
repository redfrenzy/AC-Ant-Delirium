use super::*;

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
#[test]
fn validates_kernel_ucred_and_rejects_invalid_kernel_shapes() {
    let valid = libc::ucred {
        pid: 123,
        uid: 456,
        gid: 789,
    };
    assert_eq!(
        validated_kernel_credentials(valid, std::mem::size_of::<libc::ucred>()),
        Ok(KernelPeerCredentials {
            pid: 123,
            uid: 456,
            gid: 789,
        })
    );
    assert_eq!(
        validated_kernel_credentials(
            libc::ucred {
                pid: 0,
                uid: 456,
                gid: 789,
            },
            std::mem::size_of::<libc::ucred>()
        ),
        Err(ConnectionAuthenticationError::PeerCredentials)
    );
    assert_eq!(
        validated_kernel_credentials(valid, 1),
        Err(ConnectionAuthenticationError::PeerCredentials)
    );
}

#[cfg(unix)]
#[test]
fn kernel_entropy_is_fresh_and_accepted_descriptor_is_close_on_exec() {
    let mut first = [0_u8; CONNECTION_NONCE_BYTES];
    let mut second = [0_u8; CONNECTION_NONCE_BYTES];
    fill_system_random(&mut first).expect("first kernel nonce");
    fill_system_random(&mut second).expect("second kernel nonce");
    assert!(first.iter().any(|byte| *byte != 0));
    assert_ne!(first, second);

    let (service, _client) = UnixStream::pair().expect("socketpair fixture");
    set_close_on_exec(&service).expect("close-on-exec");
    let flags = unsafe { libc::fcntl(service.as_raw_fd(), libc::F_GETFD) };
    assert!(flags >= 0);
    assert_ne!(flags & libc::FD_CLOEXEC, 0);
}
