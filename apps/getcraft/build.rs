fn main() {
    // Give GetCraft.exe its icon and version info in Explorer and the taskbar.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/getcraft.ico");
        res.set("ProductName", "GetCraft");
        res.set("FileDescription", "GetCraft");
        res.compile().expect("embedding the Windows icon failed");
    }
    println!("cargo:rerun-if-changed=../../assets/getcraft.ico");
}
