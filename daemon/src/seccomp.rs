// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The DiamaneOS Project
//! Small arm64 seccomp allowlist installed before Binder creates threads.
//! SELinux additionally controls opened paths, ioctl commands and Binder peers.
use std::io;
#[cfg(target_arch = "aarch64")]
pub fn install() -> io::Result<()> {
    use libc::{sock_filter as F, sock_fprog};
    const LD: u16 = 0x20;
    const JEQ: u16 = 0x15;
    const RET: u16 = 0x06;
    const KILL: u32 = 0x80000000;
    const ALLOW: u32 = 0x7fff0000;
    const ERRNO: u32 = 0x50000;
    let mut f = vec![
        F {
            code: LD,
            jt: 0,
            jf: 0,
            k: 4,
        },
        F {
            code: JEQ,
            jt: 1,
            jf: 0,
            k: 0xc00000b7,
        },
        F {
            code: RET,
            jt: 0,
            jf: 0,
            k: KILL,
        },
        F {
            code: LD,
            jt: 0,
            jf: 0,
            k: 0,
        },
    ];
    // socket(2) is permitted for QRTR only; Binder is a file descriptor.
    f.extend([
        F {
            code: JEQ,
            jt: 0,
            jf: 4,
            k: libc::SYS_socket as u32,
        },
        F {
            code: LD,
            jt: 0,
            jf: 0,
            k: 16,
        },
        F {
            code: JEQ,
            jt: 1,
            jf: 0,
            k: 42,
        },
        F {
            code: RET,
            jt: 0,
            jf: 0,
            k: ERRNO | libc::EPERM as u32,
        },
        F {
            code: RET,
            jt: 0,
            jf: 0,
            k: ALLOW,
        },
    ]);
    // clone is only for threads sharing this process. clone3 falls through to
    // ENOSYS so bionic/libc can use the inspectable clone call.
    f.extend([
        F {
            code: JEQ,
            jt: 0,
            jf: 4,
            k: libc::SYS_clone as u32,
        },
        F {
            code: LD,
            jt: 0,
            jf: 0,
            k: 16,
        },
        F {
            code: 0x45,
            jt: 1,
            jf: 0,
            k: libc::CLONE_THREAD as u32,
        },
        F {
            code: RET,
            jt: 0,
            jf: 0,
            k: ERRNO | libc::EPERM as u32,
        },
        F {
            code: RET,
            jt: 0,
            jf: 0,
            k: ALLOW,
        },
    ]);
    for nr in [
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_writev,
        libc::SYS_close,
        libc::SYS_openat,
        79, // __NR_newfstatat: asm-generic unistd.h
        80, // __NR_fstat: asm-generic unistd.h
        libc::SYS_readlinkat,
        libc::SYS_faccessat,
        libc::SYS_lseek,
        libc::SYS_pread64,
        libc::SYS_mmap,
        libc::SYS_mprotect,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_ioctl,
        libc::SYS_fcntl,
        libc::SYS_dup,
        libc::SYS_dup3,
        libc::SYS_bind,
        libc::SYS_getsockname,
        libc::SYS_sendto,
        libc::SYS_recvfrom,
        libc::SYS_recvmsg,
        libc::SYS_getsockopt,
        libc::SYS_setsockopt,
        libc::SYS_ppoll,
        libc::SYS_futex,
        libc::SYS_set_robust_list,
        libc::SYS_set_tid_address,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_clock_gettime,
        libc::SYS_clock_nanosleep,
        libc::SYS_nanosleep,
        libc::SYS_gettimeofday,
        libc::SYS_sched_yield,
        libc::SYS_sched_getaffinity,
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
        f.push(F {
            code: JEQ,
            jt: 0,
            jf: 1,
            k: nr as u32,
        });
        f.push(F {
            code: RET,
            jt: 0,
            jf: 0,
            k: ALLOW,
        });
    }
    f.push(F {
        code: RET,
        jt: 0,
        jf: 0,
        k: ERRNO | libc::ENOSYS as u32,
    });
    let program = sock_fprog {
        len: f.len() as u16,
        filter: f.as_mut_ptr(),
    };
    // SAFETY: kernel copies this bounded BPF program synchronously; all pointers
    // remain live until prctl returns. Thread creation happens only afterward.
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
        "qualified runtime is arm64 only",
    ))
}
