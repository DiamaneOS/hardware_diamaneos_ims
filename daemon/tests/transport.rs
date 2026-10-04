// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_runtime::transport::{classify, Fault};
use std::io;
#[test]
fn pinned_retry_peer_and_socket_errors_do_not_collapse_to_one_fatal_category() {
    for code in [libc::EAGAIN, libc::EINTR] {
        assert!(classify(&io::Error::from_raw_os_error(code)) == Fault::Retry);
    }
    for code in [libc::EPIPE, libc::ENODEV, libc::ECONNRESET] {
        assert!(classify(&io::Error::from_raw_os_error(code)) == Fault::PeerGone);
    }
    assert!(classify(&io::Error::from_raw_os_error(libc::ENETRESET)) == Fault::SocketReset);
    for code in [libc::EBADF, libc::EINVAL, libc::EPERM] {
        assert!(classify(&io::Error::from_raw_os_error(code)) == Fault::Unexpected);
    }
}
