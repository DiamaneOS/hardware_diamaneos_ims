// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_runtime::transport::{classify, exhausted, Fault};
use std::io;
#[test]
fn pinned_retry_peer_and_socket_errors_do_not_collapse_to_one_fatal_category() {
    for code in [libc::EAGAIN, libc::EINTR, libc::ENOBUFS, libc::ENOMEM] {
        assert!(classify(&io::Error::from_raw_os_error(code)) == Fault::Retry);
    }
    for code in [
        libc::EPIPE,
        libc::ENODEV,
        libc::ECONNRESET,
        libc::EHOSTUNREACH,
    ] {
        assert!(classify(&io::Error::from_raw_os_error(code)) == Fault::PeerGone);
    }
    assert!(classify(&io::Error::from_raw_os_error(libc::ENETRESET)) == Fault::SocketReset);
    for code in [libc::EBADF, libc::EINVAL, libc::EPERM] {
        assert!(classify(&io::Error::from_raw_os_error(code)) == Fault::Unexpected);
    }
}
#[test]
fn only_buffer_and_memory_exhaustion_count_as_exhaustion() {
    for code in [libc::ENOBUFS, libc::ENOMEM] {
        assert!(exhausted(&io::Error::from_raw_os_error(code)));
    }
    for code in [libc::EAGAIN, libc::EINTR, libc::EPIPE, libc::EINVAL] {
        assert!(!exhausted(&io::Error::from_raw_os_error(code)));
    }
    assert!(!exhausted(&io::Error::other("short QRTR write")));
}
