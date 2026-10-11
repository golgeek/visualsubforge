//! Controlled direct child for the Windows executable cancellation check.
//! Ignore console Ctrl-C so only the CLI's process supervisor can terminate us.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

fn main() {
    // SAFETY: null selects the documented per-process ignore flag. No pointer,
    // callback or native resource is retained, and this helper has no children.
    assert_ne!(unsafe { SetConsoleCtrlHandler(None, 1) }, 0);
    std::fs::write(
        std::env::var_os("VSF_TEST_CHILD_PID").expect("PID output path"),
        std::process::id().to_string(),
    )
    .expect("write helper PID");
    std::thread::sleep(std::time::Duration::from_secs(60));
}
