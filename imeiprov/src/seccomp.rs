// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Small arm64 seccomp allowlist for the one-shot provisioning tool, installed
//! after the partition read and before any QRTR work. SELinux independently
//! controls the opened paths and the reachable sockets. `socket(2)` is limited
//! to QRTR and the local AF_UNIX log socket; everything unlisted returns ENOSYS.

use std::io;

#[cfg(target_arch = "aarch64")]
pub fn install() -> io::Result<()> {
    use libc::{sock_filter as F, sock_fprog};
    const LD: u16 = 0x20;
    const JEQ: u16 = 0x15;
    const RET: u16 = 0x06;
    const AUDIT_ARCH_AARCH64: u32 = 0xc00000b7;
    const ARCH_OFFSET: u32 = 4;
    const SYSCALL_OFFSET: u32 = 0;
    const ARG0_OFFSET: u32 = 16;
    const AF_QIPCRTR: u32 = 42;
    const AF_UNIX: u32 = 1; // liblog connects a local datagram socket to logd.
    const NR_NEWFSTATAT: u32 = 79;
    const NR_FSTAT: u32 = 80;
    const KILL: u32 = 0x8000_0000;
    const ALLOW: u32 = 0x7fff_0000;
    const ERRNO: u32 = 0x5_0000;
    let allow = |k: u32| F {
        code: RET,
        jt: 0,
        jf: 0,
        k,
    };
    let ld = |k: u32| F {
        code: LD,
        jt: 0,
        jf: 0,
        k,
    };
    let jeq = |k: u32, jt: u8, jf: u8| F {
        code: JEQ,
        jt,
        jf,
        k,
    };
    let mut f = vec![
        ld(ARCH_OFFSET),
        jeq(AUDIT_ARCH_AARCH64, 1, 0),
        allow(KILL),
        ld(SYSCALL_OFFSET),
    ];
    // socket(2): permit only AF_QIPCRTR and the AF_UNIX log socket.
    f.extend([
        jeq(libc::SYS_socket as u32, 0, 5),
        ld(ARG0_OFFSET),
        jeq(AF_QIPCRTR, 2, 0),
        jeq(AF_UNIX, 1, 0),
        allow(ERRNO | libc::EPERM as u32),
        allow(ALLOW),
    ]);
    f.push(ld(SYSCALL_OFFSET));
    for nr in [
        libc::SYS_connect,
        libc::SYS_bind,
        libc::SYS_getsockname,
        libc::SYS_sendto,
        libc::SYS_recvfrom,
        libc::SYS_recvmsg,
        libc::SYS_getsockopt,
        libc::SYS_setsockopt,
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_writev,
        libc::SYS_pread64,
        libc::SYS_lseek,
        libc::SYS_close,
        libc::SYS_openat,
        NR_NEWFSTATAT as libc::c_long,
        NR_FSTAT as libc::c_long,
        libc::SYS_readlinkat,
        libc::SYS_faccessat,
        libc::SYS_ioctl,
        libc::SYS_fcntl,
        libc::SYS_dup,
        libc::SYS_dup3,
        libc::SYS_mmap,
        libc::SYS_mprotect,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_ppoll,
        libc::SYS_futex,
        libc::SYS_nanosleep,
        libc::SYS_clock_nanosleep,
        libc::SYS_clock_gettime,
        libc::SYS_gettimeofday,
        libc::SYS_sched_yield,
        libc::SYS_sched_getaffinity,
        libc::SYS_set_robust_list,
        libc::SYS_set_tid_address,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_getpid,
        libc::SYS_gettid,
        libc::SYS_getuid,
        libc::SYS_geteuid,
        libc::SYS_getgid,
        libc::SYS_getegid,
        libc::SYS_getrandom,
        libc::SYS_uname,
        libc::SYS_prlimit64,
        libc::SYS_prctl,
        libc::SYS_tgkill,
        libc::SYS_exit,
        libc::SYS_exit_group,
    ] {
        f.push(jeq(nr as u32, 0, 1));
        f.push(allow(ALLOW));
    }
    f.push(allow(ERRNO | libc::ENOSYS as u32));
    let program = sock_fprog {
        len: f.len() as u16,
        filter: f.as_mut_ptr(),
    };
    // SAFETY: the kernel copies this bounded BPF program synchronously; the
    // pointers stay live until prctl returns.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::prctl(libc::PR_SET_SECCOMP, 2, &program as *const sock_fprog) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_arch = "aarch64"))]
pub fn install() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "imeiprovd runs on arm64 only",
    ))
}
