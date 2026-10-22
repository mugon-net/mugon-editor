//! Embeds the Mugon logo as the Windows executable icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/icon.ico");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed the executable icon: {error}");
    }
}
