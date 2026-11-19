#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("ProductName", "Lunula Terminal");
    res.set("FileDescription", "Lunula Terminal - Modern GPU-Accelerated Terminal Emulator");
    res.set("LegalCopyright", "Copyright (c) 2026 Lunula Project");
    if let Err(e) = res.compile() {
        eprintln!("Warning: Failed to compile windows resource: {}", e);
    }
}

#[cfg(not(windows))]
fn main() {}
