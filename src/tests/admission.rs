use super::*;

#[test]
fn board_contract_rejects_unsupported_backends() {
    #[cfg(not(all(feature = "nx5", target_arch = "arm", target_pointer_width = "32")))]
    assert!(matches!(
        backends::require(Backend::Nx5),
        Err(Error::DisplayNotAdmitted { .. })
    ));
    assert!(matches!(
        backends::require(Backend::Fbdev),
        Err(Error::DisplayNotAdmitted { .. })
    ));
}

#[test]
fn lock_io_errors_remain_io_errors() {
    use std::os::{fd::AsFd, unix::fs::OpenOptionsExt};
    let descriptor = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open("/dev/null")
        .unwrap();
    assert!(matches!(
        backends::lock_exclusive(descriptor.as_fd(), "lock test descriptor"),
        Err(Error::Io { operation: "lock test descriptor", source }) if source.raw_os_error() == Some(libc::EBADF)
    ));
}
