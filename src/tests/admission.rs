use super::*;

#[test]
fn board_contract_rejects_unadmitted_profiles() {
    assert!(matches!(
        backends::require(Backend::Nx5, std::path::Path::new("/does/not/exist")),
        Err(Error::DisplayNotAdmitted { .. })
    ));
    assert!(matches!(
        backends::require(Backend::Fbdev, std::path::Path::new("/dev/fb0")),
        Err(Error::DisplayNotAdmitted { .. })
    ));
}
