fn main() {
    println!("cargo:rerun-if-changed=assets/verilog-ide.ico");
    #[cfg(windows)]
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    let icon = "assets/verilog-ide.ico";
    if !std::path::Path::new(icon).is_file() {
        println!("cargo:warning=missing {icon}; the executable will use the default icon");
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon);
    res.set("FileDescription", "Verilog IDE");
    res.set("ProductName", "Verilog IDE");
    res.set("OriginalFilename", "verilog-ide.exe");
    if let Err(err) = res.compile() {
        println!("cargo:warning=could not embed the application icon: {err}");
    }
}
