// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Minimal QMI-over-QRTR client: bind an ephemeral port, look up a modem service
//! by (service, instance) and exchange single request/response datagrams. The
//! control-packet and QMI framing come from `diamaneos_ims_dcm`.

pub use diamaneos_ims_dcm::engine::Peer;
use diamaneos_ims_dcm::qrtr::{Control, CTRL_PORT, DEL_LOOKUP, NEW_LOOKUP, NEW_SERVER};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};
use std::{io, mem};

const AF_QIPCRTR: libc::c_int = 42;
const RECV_BUF: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
struct Address {
    family: u16,
    reserved: u16,
    node: u32,
    port: u32,
}

pub struct Client {
    fd: OwnedFd,
    local: Peer,
}

impl Client {
    /// Bind an ephemeral QRTR port on the local node.
    pub fn bind() -> io::Result<Self> {
        // SAFETY: socket(2) returns an owned fd; the address is the 12-byte
        // sockaddr_qrtr whose length the kernel validates.
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
        // The kernel knows this socket's node; read it before binding port 0.
        let mut len = mem::size_of::<Address>() as libc::socklen_t;
        if unsafe { libc::getsockname(raw, (&mut a as *mut Address).cast(), &mut len) } < 0 {
            return Err(io::Error::last_os_error());
        }
        if len as usize != mem::size_of::<Address>() || a.family != AF_QIPCRTR as u16 {
            return Err(io::Error::other("invalid QRTR local address"));
        }
        a.port = 0;
        if unsafe {
            libc::bind(
                raw,
                (&a as *const Address).cast(),
                mem::size_of::<Address>() as libc::socklen_t,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        len = mem::size_of::<Address>() as libc::socklen_t;
        if unsafe { libc::getsockname(raw, (&mut a as *mut Address).cast(), &mut len) } < 0
            || len as usize != mem::size_of::<Address>()
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
        })
    }

    fn send(&self, to: Peer, bytes: &[u8]) -> io::Result<()> {
        let addr = Address {
            family: AF_QIPCRTR as u16,
            reserved: 0,
            node: to.node,
            port: to.port,
        };
        // SAFETY: a borrowed buffer and a fixed-size address passed to one
        // synchronous sendto; no pointer escapes.
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

    fn recv(&self, timeout: Duration) -> io::Result<Option<(Peer, Vec<u8>)>> {
        let mut p = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = timeout.as_millis().min(1000) as libc::c_int;
        let rc = unsafe { libc::poll(&mut p, 1, ms) };
        if rc < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(e)
            };
        }
        if rc == 0 || p.revents & libc::POLLIN == 0 {
            return Ok(None);
        }
        let mut bytes = [0u8; RECV_BUF];
        let mut a = Address {
            family: 0,
            reserved: 0,
            node: 0,
            port: 0,
        };
        let mut from_len = mem::size_of::<Address>() as libc::socklen_t;
        // SAFETY: recvfrom writes at most RECV_BUF bytes into the owned buffer
        // and the address into the sized struct; lengths are checked after.
        let n = unsafe {
            libc::recvfrom(
                self.fd.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT,
                (&mut a as *mut Address).cast(),
                &mut from_len,
            )
        };
        if n < 0 {
            let e = io::Error::last_os_error();
            return if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                Ok(None)
            } else {
                Err(e)
            };
        }
        if from_len as usize != mem::size_of::<Address>() || a.family != AF_QIPCRTR as u16 {
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

    /// Look up a service by (service, instance). Sends one NEW_LOOKUP and waits
    /// for a matching NEW_SERVER notification until the deadline. Returns the
    /// server's peer, or None if it never appears (e.g. the modem is not up).
    pub fn lookup(
        &self,
        service: u32,
        instance: u32,
        overall: Duration,
    ) -> io::Result<Option<Peer>> {
        self.control(Control {
            command: NEW_LOOKUP,
            words: [service, instance, 0, 0],
        })?;
        let deadline = Instant::now() + overall;
        let mut found = None;
        while Instant::now() < deadline {
            let Some((from, data)) = self.recv(Duration::from_millis(200))? else {
                continue;
            };
            if from.port != CTRL_PORT {
                continue;
            }
            if let Some(c) = Control::decode(&data) {
                if c.command == NEW_SERVER && c.words[0] == service && c.words != [0; 4] {
                    found = Some(Peer {
                        node: c.words[2],
                        port: c.words[3],
                    });
                    break;
                }
            }
        }
        let _ = self.control(Control {
            command: DEL_LOOKUP,
            words: [service, instance, 0, 0],
        });
        Ok(found)
    }

    /// Send one request to `server` and return the first datagram it sends back,
    /// retrying the send up to `tries` times.
    pub fn transact(
        &self,
        server: Peer,
        request: &[u8],
        per_try: Duration,
        tries: u32,
    ) -> io::Result<Option<Vec<u8>>> {
        for _ in 0..tries.max(1) {
            self.send(server, request)?;
            let deadline = Instant::now() + per_try;
            while Instant::now() < deadline {
                let Some((from, data)) = self.recv(Duration::from_millis(200))? else {
                    continue;
                };
                if from.node == server.node && from.port == server.port {
                    return Ok(Some(data));
                }
                // Ignore control notices and stray datagrams; keep waiting.
            }
        }
        Ok(None)
    }
}
