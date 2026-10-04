// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Small FFI boundary. Owns one datagram fd; never opens Internet sockets.
use diamaneos_ims_dcm::{engine::Peer, protocol::MAX_DATAGRAM, qrtr::*};
use std::{
    io, mem,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::{Duration, Instant},
};
const AF_QIPCRTR: libc::c_int = 42;
#[repr(C)]
#[derive(Clone, Copy)]
struct Address {
    family: u16,
    reserved: u16,
    node: u32,
    port: u32,
}
pub enum Publication {
    Ready,
    Conflict,
}
pub struct Qrtr {
    fd: OwnedFd,
    local: Peer,
    published: bool,
}
impl Qrtr {
    pub fn bind() -> io::Result<Self> {
        // SAFETY: socket returns an owned fd on success. Address is the Linux UAPI
        // sockaddr_qrtr (12 bytes); both sizes are checked by the kernel.
        let raw = unsafe {
            libc::socket(
                AF_QIPCRTR,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                0,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut a = Address {
            family: AF_QIPCRTR as u16,
            reserved: 0,
            node: 0,
            port: 0,
        };
        // A new QRTR socket already knows its local node. qrtr_bind rejects
        // any other node, including zero when the kernel's local node is one.
        // Ask the kernel before requesting an ephemeral port; never infer the
        // AP node from the modem node or a device-specific constant.
        let mut len = mem::size_of::<Address>() as libc::socklen_t;
        let rc = unsafe { libc::getsockname(raw, (&mut a as *mut Address).cast(), &mut len) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        if len as usize != mem::size_of::<Address>() || a.family != AF_QIPCRTR as u16 {
            return Err(io::Error::other("invalid QRTR local address"));
        }
        a.port = 0;
        let rc = unsafe {
            libc::bind(
                raw,
                (&a as *const Address).cast(),
                mem::size_of::<Address>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        len = mem::size_of::<Address>() as libc::socklen_t;
        let rc = unsafe { libc::getsockname(raw, (&mut a as *mut Address).cast(), &mut len) };
        if rc < 0
            || len as usize != mem::size_of::<Address>()
            || a.family != AF_QIPCRTR as u16
            || a.port == 0
        {
            return Err(io::Error::other("invalid QRTR bind"));
        }
        Ok(Self {
            fd,
            local: Peer {
                node: a.node,
                port: a.port,
            },
            published: false,
        })
    }
    pub fn local(&self) -> Peer {
        self.local
    }
    pub fn send(&self, to: Peer, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_DATAGRAM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "oversized packet",
            ));
        }
        let addr = Address {
            family: AF_QIPCRTR as u16,
            reserved: 0,
            node: to.node,
            port: to.port,
        };
        let rc = unsafe {
            libc::sendto(
                self.fd.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT,
                (&addr as *const Address).cast(),
                mem::size_of::<Address>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        if rc as usize != bytes.len() {
            return Err(io::Error::other("short QRTR write"));
        }
        Ok(())
    }
    pub fn receive(&self, timeout: Duration) -> io::Result<Option<(Peer, Vec<u8>)>> {
        let mut p = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut p, 1, timeout.as_millis().min(1000) as i32) };
        if rc < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(e);
        }
        if rc == 0 {
            return Ok(None);
        }
        if p.revents & libc::POLLNVAL != 0 {
            return Err(io::Error::from_raw_os_error(libc::EBADF));
        }
        if p.revents & libc::POLLHUP != 0 {
            return Err(io::Error::from_raw_os_error(libc::ENETRESET));
        }
        if p.revents & libc::POLLERR != 0 {
            let mut error: libc::c_int = 0;
            let mut size = mem::size_of_val(&error) as libc::socklen_t;
            // SAFETY: fixed integer output and matching length; this only reads
            // the kernel's queued socket error, it changes no routing or policy.
            let result = unsafe {
                libc::getsockopt(
                    self.fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&mut error as *mut libc::c_int).cast(),
                    &mut size,
                )
            };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            if size as usize != mem::size_of_val(&error) {
                return Err(io::Error::other("socket error width"));
            }
            if error != 0 {
                return Err(io::Error::from_raw_os_error(error));
            }
            if p.revents & libc::POLLIN == 0 {
                return Ok(None);
            }
        }
        let mut bytes = [0u8; MAX_DATAGRAM];
        let mut a = Address {
            family: 0,
            reserved: 0,
            node: 0,
            port: 0,
        };
        let mut len = mem::size_of::<Address>() as libc::socklen_t;
        let n = unsafe {
            libc::recvfrom(
                self.fd.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT | libc::MSG_TRUNC,
                (&mut a as *mut Address).cast(),
                &mut len,
            )
        };
        if n < 0 {
            let e = io::Error::last_os_error();
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                return Ok(None);
            }
            return Err(e);
        }
        if n as usize > bytes.len()
            || len as usize != mem::size_of::<Address>()
            || a.family != AF_QIPCRTR as u16
        {
            return Ok(None);
        }
        Ok(Some((
            Peer {
                node: a.node,
                port: a.port,
            },
            bytes[..n as usize].to_vec(),
        )))
    }
    fn control(&self, c: Control) -> io::Result<()> {
        self.send(
            Peer {
                node: self.local.node,
                port: CTRL_PORT,
            },
            &c.encode(),
        )
    }
    pub fn publish(&mut self) -> io::Result<Publication> {
        self.control(Control::lookup(NEW_LOOKUP))?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut complete = false;
        while Instant::now() < deadline {
            if let Some((peer, data)) = self.receive(Duration::from_millis(100))? {
                if peer
                    != (Peer {
                        node: self.local.node,
                        port: CTRL_PORT,
                    })
                {
                    continue;
                }
                if let Some(c) = Control::decode(&data) {
                    if c.conflicting_server() {
                        self.control(Control::lookup(DEL_LOOKUP))?;
                        return Ok(Publication::Conflict);
                    }
                    if c.lookup_complete() {
                        complete = true;
                        break;
                    }
                }
            }
        }
        if !complete {
            self.control(Control::lookup(DEL_LOOKUP))?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "QRTR lookup incomplete",
            ));
        }
        self.control(Control::server(NEW_SERVER, self.local))?;
        self.published = true;
        Ok(Publication::Ready)
    }
}
impl Drop for Qrtr {
    fn drop(&mut self) {
        if self.published {
            let _ = self.control(Control::server(DEL_SERVER, self.local));
        }
        let _ = self.control(Control::lookup(DEL_LOOKUP));
    }
}
