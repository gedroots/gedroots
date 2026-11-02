//! Windows: the application icon and version inside `gedroots.exe` (Explorer, Start menu, taskbar,
//! installer shortcuts).

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/windows/gedroots.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/windows/gedroots.ico")
            .set("ProductName", "GEDRoots")
            .set("FileDescription", "GEDRoots");
        res.compile().expect("Windows resources");
    }
}
