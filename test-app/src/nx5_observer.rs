//! NX5-only composed-output sampling; normal UI checks use Slint MCP.

pub fn start(samples: String) -> std::thread::JoinHandle<i32> {
    std::thread::spawn(move || {
        // Loading this SDK in a second process resets the global VO/SYS hardware.
        unsafe {
            let library = libloading::Library::new("/run/slint-bridge-display/libobserver.so")
                .expect("load observer");
            let function = library
                .get::<unsafe extern "C" fn(i32, *const *const libc::c_char) -> i32>(
                    b"bridge_wbc_observe\0",
                )
                .expect("observer function");
            let argument = std::ffi::CString::new(samples).unwrap();
            let args = [c"observer".as_ptr(), argument.as_ptr()];
            let result = function(2, args.as_ptr());
            eprintln!("display: observation result={result}");
            result
        }
    })
}
