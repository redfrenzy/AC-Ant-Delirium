#[cfg(unix)]
use std::mem;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::time::{SystemTime, UNIX_EPOCH};

use ac_ant_delirium::libres::System_Random_unit::fill_system_random;
use ac_ant_delirium::structs::{
    CapabilityAuthorizationOutcome, CapabilityAuthorizationRequest, CapabilityChannel,
    ProjectDatabase, TransactionalStorageError, TransactionalStorageResult, CONNECTION_NONCE_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KernelPeerCredentials {
    pub(crate) pid: u32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ConnectionAuthentication {
    peer: KernelPeerCredentials,
    connection_nonce: [u8; CONNECTION_NONCE_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionAuthenticationError {
    PeerCredentials,
    Entropy,
    Descriptor,
}

impl ConnectionAuthentication {
    #[cfg(unix)]
    pub(crate) fn capture(connection: &UnixStream) -> Result<Self, ConnectionAuthenticationError> {
        set_close_on_exec(connection)?;
        let peer = kernel_peer_credentials(connection)?;
        let mut connection_nonce = [0_u8; CONNECTION_NONCE_BYTES];
        fill_system_random(&mut connection_nonce)
            .map_err(|_| ConnectionAuthenticationError::Entropy)?;
        if connection_nonce.iter().all(|byte| *byte == 0) {
            return Err(ConnectionAuthenticationError::Entropy);
        }
        Ok(Self {
            peer,
            connection_nonce,
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn capture(_connection: &()) -> Result<Self, ConnectionAuthenticationError> {
        Err(ConnectionAuthenticationError::PeerCredentials)
    }

    // The typed request parser introduced after the authorization boundary will be
    // the first production caller. Keeping the trusted peer/channel/nonce outside
    // the request prevents those values from ever becoming client-controlled.
    #[allow(dead_code)]
    pub(crate) fn authorize(
        &self,
        registry: &ProjectDatabase,
        channel: CapabilityChannel,
        request: &CapabilityAuthorizationRequest,
    ) -> TransactionalStorageResult<CapabilityAuthorizationOutcome> {
        registry.authorize_capability(
            self.peer.uid,
            channel,
            &self.connection_nonce,
            trusted_unix_time()?,
            request,
        )
    }
}

fn trusted_unix_time() -> TransactionalStorageResult<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| TransactionalStorageError::Integrity("service clock is invalid".to_string()))?
        .as_secs();
    i64::try_from(seconds)
        .map_err(|_| TransactionalStorageError::Integrity("service clock is invalid".to_string()))
}

#[cfg(unix)]
fn kernel_peer_credentials(
    connection: &UnixStream,
) -> Result<KernelPeerCredentials, ConnectionAuthenticationError> {
    let mut credentials: libc::ucred = unsafe { mem::zeroed() };
    let mut length = mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            connection.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(ConnectionAuthenticationError::PeerCredentials);
    }
    validated_kernel_credentials(credentials, length as usize)
}

#[cfg(unix)]
fn validated_kernel_credentials(
    credentials: libc::ucred,
    returned_length: usize,
) -> Result<KernelPeerCredentials, ConnectionAuthenticationError> {
    if returned_length != mem::size_of::<libc::ucred>() || credentials.pid <= 0 {
        return Err(ConnectionAuthenticationError::PeerCredentials);
    }
    Ok(KernelPeerCredentials {
        pid: u32::try_from(credentials.pid)
            .map_err(|_| ConnectionAuthenticationError::PeerCredentials)?,
        uid: credentials.uid,
        gid: credentials.gid,
    })
}

#[cfg(unix)]
#[cfg(unix)]
fn set_close_on_exec(connection: &UnixStream) -> Result<(), ConnectionAuthenticationError> {
    let raw_fd = connection.as_raw_fd();
    let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(raw_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(ConnectionAuthenticationError::Descriptor);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/Service_Authentication_test.rs"]
mod service_authentication_tests;
