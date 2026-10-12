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
