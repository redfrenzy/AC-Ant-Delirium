use std::collections::HashSet;
use std::ffi::OsStr;
#[cfg(unix)]
use std::io;

#[cfg(unix)]
use std::mem;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::net::UnixListener;

use crate::public_output::{OutputRouter, PublicError};
use crate::service_authentication::ConnectionAuthentication;
use crate::service_authority::VerifiedServiceAuthority;
use crate::service_storage::ProtectedServiceStorage;

const LISTEN_PID_ENV: &str = "LISTEN_PID";
const LISTEN_FDS_ENV: &str = "LISTEN_FDS";
const LISTEN_FDNAMES_ENV: &str = "LISTEN_FDNAMES";
const SYSTEMD_FIRST_DESCRIPTOR: i32 = 3;
const EXPECTED_DESCRIPTOR_COUNT: u32 = 2;
const LIMITED_DESCRIPTOR_NAME: &str = "agent";
const ADMINISTRATIVE_DESCRIPTOR_NAME: &str = "admin";
const LIMITED_MAGIC: [u8; 4] = *b"ADL1";
const ADMINISTRATIVE_MAGIC: [u8; 4] = *b"ADA1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ServiceChannelKind {
    Limited,
    Administrative,
}

impl ServiceChannelKind {
    fn from_descriptor_name(name: &str) -> Option<Self> {
        match name {
            LIMITED_DESCRIPTOR_NAME => Some(Self::Limited),
            ADMINISTRATIVE_DESCRIPTOR_NAME => Some(Self::Administrative),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivatedDescriptorSpec {
    kind: ServiceChannelKind,
    raw_fd: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceChannelError {
    ActivationContract,
    DescriptorContract,
}

#[cfg(unix)]
pub(crate) struct ActivatedServiceChannels {
    _limited: LimitedServiceListener,
    _administrative: AdministrativeServiceListener,
}

#[cfg(not(unix))]
pub(crate) struct ActivatedServiceChannels;

#[cfg(unix)]
struct LimitedServiceListener {
    listener: UnixListener,
    parser: LimitedChannelParser,
}

#[cfg(unix)]
struct AdministrativeServiceListener {
    listener: UnixListener,
    parser: AdministrativeChannelParser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LimitedChannelParser {
    magic: [u8; 4],
}

impl LimitedChannelParser {
    fn new() -> Self {
        Self {
            magic: LIMITED_MAGIC,
        }
    }

    #[cfg(test)]
    fn accepts_magic(self, magic: [u8; 4]) -> bool {
        magic == self.magic
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AdministrativeChannelParser {
    magic: [u8; 4],
}

impl AdministrativeChannelParser {
    fn new() -> Self {
        Self {
            magic: ADMINISTRATIVE_MAGIC,
        }
    }

    #[cfg(test)]
    fn accepts_magic(self, magic: [u8; 4]) -> bool {
        magic == self.magic
    }
}

pub(crate) fn run_service_channels(authority: VerifiedServiceAuthority) -> ! {
    let channels = ActivatedServiceChannels::from_systemd()
        .unwrap_or_else(|_| OutputRouter::exit(PublicError::Configuration));
    let storage = crate::service_storage::initialize_configured_service_storage(&authority)
        .unwrap_or_else(|_| OutputRouter::exit(PublicError::Configuration));
    channels.reject_unauthenticated_connections(storage)
}

impl ActivatedServiceChannels {
    #[cfg(unix)]
    fn from_systemd() -> Result<Self, ServiceChannelError> {
        let listen_pid = std::env::var_os(LISTEN_PID_ENV);
        let listen_fds = std::env::var_os(LISTEN_FDS_ENV);
        let listen_fdnames = std::env::var_os(LISTEN_FDNAMES_ENV);
        clear_activation_environment();
        let descriptors = parse_activation_environment(
            std::process::id(),
            listen_pid.as_deref(),
            listen_fds.as_deref(),
            listen_fdnames.as_deref(),
        )?;
        Self::claim(descriptors)
    }

    #[cfg(not(unix))]
    fn from_systemd() -> Result<Self, ServiceChannelError> {
        Err(ServiceChannelError::ActivationContract)
    }

    #[cfg(unix)]
    fn claim(descriptors: [ActivatedDescriptorSpec; 2]) -> Result<Self, ServiceChannelError> {
        let raw_fds = descriptors
            .iter()
            .map(|descriptor| descriptor.raw_fd)
            .collect::<HashSet<_>>();
        if raw_fds.len() != descriptors.len() || raw_fds.iter().any(|fd| *fd < 0) {
            return Err(ServiceChannelError::DescriptorContract);
        }
        for raw_fd in &raw_fds {
            if unsafe { libc::fcntl(*raw_fd, libc::F_GETFD) } < 0 {
                return Err(ServiceChannelError::DescriptorContract);
            }
        }

        // SAFETY: uniqueness and non-negative values are checked before constructing
        // OwnedFd, and F_GETFD proved that each descriptor is open. The socket-
        // activation contract transfers ownership to this process exactly once.
        let mut owned = descriptors.map(|descriptor| {
            Some((descriptor.kind, unsafe {
                OwnedFd::from_raw_fd(descriptor.raw_fd)
            }))
        });
        let mut listener_identities = Vec::with_capacity(EXPECTED_DESCRIPTOR_COUNT as usize);
        for entry in &owned {
            let (_, descriptor) = entry
                .as_ref()
                .ok_or(ServiceChannelError::DescriptorContract)?;
            listener_identities.push(validate_unix_stream_listener(descriptor)?);
            set_close_on_exec(descriptor)?;
        }
        require_distinct_listener_identities(&listener_identities)?;

        let limited_index = owned
            .iter()
            .position(|entry| {
                entry
                    .as_ref()
                    .is_some_and(|(kind, _)| *kind == ServiceChannelKind::Limited)
            })
            .ok_or(ServiceChannelError::ActivationContract)?;
        let (limited_kind, limited_fd) = owned[limited_index]
            .take()
            .ok_or(ServiceChannelError::ActivationContract)?;
        let administrative_index = owned
            .iter()
            .position(|entry| {
                entry
                    .as_ref()
                    .is_some_and(|(kind, _)| *kind == ServiceChannelKind::Administrative)
            })
            .ok_or(ServiceChannelError::ActivationContract)?;
        let (administrative_kind, administrative_fd) = owned[administrative_index]
            .take()
            .ok_or(ServiceChannelError::ActivationContract)?;
        if limited_kind != ServiceChannelKind::Limited
            || administrative_kind != ServiceChannelKind::Administrative
        {
            return Err(ServiceChannelError::ActivationContract);
        }

        Ok(Self {
            _limited: LimitedServiceListener {
                listener: UnixListener::from(limited_fd),
                parser: LimitedChannelParser::new(),
            },
            _administrative: AdministrativeServiceListener {
                listener: UnixListener::from(administrative_fd),
                parser: AdministrativeChannelParser::new(),
            },
        })
    }

    #[cfg(unix)]
    fn reject_unauthenticated_connections(self, storage: ProtectedServiceStorage) -> ! {
        let mut listeners = [
            libc::pollfd {
                fd: self._limited.listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self._administrative.listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            let ready = unsafe { libc::poll(listeners.as_mut_ptr(), listeners.len() as _, -1) };
            if ready < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                OutputRouter::exit(PublicError::Configuration);
            }
            let limited_events = listeners[0].revents;
            let administrative_events = listeners[1].revents;
            for listener in &mut listeners {
                if listener.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    OutputRouter::exit(PublicError::Configuration);
                }
                listener.revents = 0;
            }
            if limited_events & libc::POLLIN != 0 {
                self._limited.reject_one(&storage);
            }
            if administrative_events & libc::POLLIN != 0 {
                self._administrative.reject_one(&storage);
            }
        }
    }

    #[cfg(not(unix))]
    fn reject_unauthenticated_connections(self, _storage: ProtectedServiceStorage) -> ! {
        OutputRouter::exit(PublicError::Configuration)
    }
}

#[cfg(unix)]
impl LimitedServiceListener {
    fn reject_one(&self, storage: &ProtectedServiceStorage) {
        match self.listener.accept() {
            Ok((connection, _address)) => {
                let authentication = ConnectionAuthentication::capture(&connection)
                    .unwrap_or_else(|_| OutputRouter::exit(PublicError::Configuration));
                let _expected_magic = self.parser.magic;
                let _authorization_boundary = (authentication, storage.registry());
                drop(connection);
            }
            Err(_) => OutputRouter::exit(PublicError::Configuration),
        }
    }
}

#[cfg(unix)]
impl AdministrativeServiceListener {
    fn reject_one(&self, storage: &ProtectedServiceStorage) {
        match self.listener.accept() {
            Ok((connection, _address)) => {
                let authentication = ConnectionAuthentication::capture(&connection)
                    .unwrap_or_else(|_| OutputRouter::exit(PublicError::Configuration));
                let _expected_magic = self.parser.magic;
                let _authorization_boundary = (authentication, storage.registry());
                drop(connection);
            }
            Err(_) => OutputRouter::exit(PublicError::Configuration),
        }
    }
}

fn parse_activation_environment(
    current_pid: u32,
    listen_pid: Option<&OsStr>,
    listen_fds: Option<&OsStr>,
    listen_fdnames: Option<&OsStr>,
) -> Result<[ActivatedDescriptorSpec; 2], ServiceChannelError> {
    let listen_pid = parse_decimal_u32(listen_pid)?;
    if listen_pid != current_pid {
        return Err(ServiceChannelError::ActivationContract);
    }
    if parse_decimal_u32(listen_fds)? != EXPECTED_DESCRIPTOR_COUNT {
        return Err(ServiceChannelError::ActivationContract);
    }
    let names = listen_fdnames
        .and_then(OsStr::to_str)
        .ok_or(ServiceChannelError::ActivationContract)?
        .split(':')
        .collect::<Vec<_>>();
    if names.len() != EXPECTED_DESCRIPTOR_COUNT as usize {
        return Err(ServiceChannelError::ActivationContract);
    }

    let mut kinds = HashSet::new();
    let mut descriptors = Vec::with_capacity(EXPECTED_DESCRIPTOR_COUNT as usize);
    for (offset, name) in names.into_iter().enumerate() {
        let kind = ServiceChannelKind::from_descriptor_name(name)
            .ok_or(ServiceChannelError::ActivationContract)?;
        if !kinds.insert(kind) {
            return Err(ServiceChannelError::ActivationContract);
        }
        let raw_fd = SYSTEMD_FIRST_DESCRIPTOR
            .checked_add(offset as i32)
            .ok_or(ServiceChannelError::ActivationContract)?;
        descriptors.push(ActivatedDescriptorSpec { kind, raw_fd });
    }
    descriptors
        .try_into()
        .map_err(|_| ServiceChannelError::ActivationContract)
}

fn parse_decimal_u32(value: Option<&OsStr>) -> Result<u32, ServiceChannelError> {
    let value = value
        .and_then(OsStr::to_str)
        .ok_or(ServiceChannelError::ActivationContract)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ServiceChannelError::ActivationContract);
    }
    value
        .parse()
        .map_err(|_| ServiceChannelError::ActivationContract)
}

fn clear_activation_environment() {
    for name in [LISTEN_PID_ENV, LISTEN_FDS_ENV, LISTEN_FDNAMES_ENV] {
        std::env::remove_var(name);
    }
}

#[cfg(unix)]
fn validate_unix_stream_listener(
    descriptor: &OwnedFd,
) -> Result<ListenerIdentity, ServiceChannelError> {
    let raw_fd = descriptor.as_raw_fd();
    let mut socket_type: libc::c_int = 0;
    let mut socket_type_len = mem::size_of::<libc::c_int>() as libc::socklen_t;
    let socket_type_result = unsafe {
        libc::getsockopt(
            raw_fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut socket_type as *mut libc::c_int).cast(),
            &mut socket_type_len,
        )
    };
    if socket_type_result != 0 {
        return Err(ServiceChannelError::DescriptorContract);
    }

    let mut accepting: libc::c_int = 0;
    let mut accepting_len = mem::size_of::<libc::c_int>() as libc::socklen_t;
    let accepting_result = unsafe {
        libc::getsockopt(
            raw_fd,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut accepting as *mut libc::c_int).cast(),
            &mut accepting_len,
        )
    };
    if accepting_result != 0 {
        return Err(ServiceChannelError::DescriptorContract);
    }

    let mut address: libc::sockaddr_storage = unsafe { mem::zeroed() };
    let mut address_len = mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    let address_result = unsafe {
        libc::getsockname(
            raw_fd,
            (&mut address as *mut libc::sockaddr_storage).cast(),
            &mut address_len,
        )
    };
    if address_result != 0 {
        return Err(ServiceChannelError::DescriptorContract);
    }
    let unix_path_offset = mem::offset_of!(libc::sockaddr_un, sun_path);
    let has_filesystem_address = address_len as usize > unix_path_offset
        && unsafe { (*(std::ptr::addr_of!(address).cast::<libc::sockaddr_un>())).sun_path[0] != 0 };
    validate_listener_facts(ListenerFacts {
        socket_type,
        accepting,
        address_family: address.ss_family as libc::c_int,
        has_filesystem_address,
    })?;

    let mut metadata: libc::stat = unsafe { mem::zeroed() };
    if unsafe { libc::fstat(raw_fd, &mut metadata) } != 0 {
        return Err(ServiceChannelError::DescriptorContract);
    }
    Ok(ListenerIdentity {
        device: metadata.st_dev as u64,
        inode: metadata.st_ino as u64,
    })
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListenerFacts {
    socket_type: libc::c_int,
    accepting: libc::c_int,
    address_family: libc::c_int,
    has_filesystem_address: bool,
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ListenerIdentity {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
fn require_distinct_listener_identities(
    identities: &[ListenerIdentity],
) -> Result<(), ServiceChannelError> {
    if identities.len() != EXPECTED_DESCRIPTOR_COUNT as usize
        || identities.iter().copied().collect::<HashSet<_>>().len() != identities.len()
    {
        return Err(ServiceChannelError::DescriptorContract);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_listener_facts(facts: ListenerFacts) -> Result<(), ServiceChannelError> {
    if facts.socket_type != libc::SOCK_STREAM
        || facts.accepting != 1
        || facts.address_family != libc::AF_UNIX
        || !facts.has_filesystem_address
    {
        return Err(ServiceChannelError::DescriptorContract);
    }
    Ok(())
}

#[cfg(unix)]
fn set_close_on_exec(descriptor: &OwnedFd) -> Result<(), ServiceChannelError> {
    let raw_fd = descriptor.as_raw_fd();
    let flags = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(raw_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(ServiceChannelError::DescriptorContract);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/Service_Channel_test.rs"]
mod service_channel_tests;
