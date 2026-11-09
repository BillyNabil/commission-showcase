fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app/icon.ico");
        res.set("ProductName", "Commission Showcase");
        res.set("FileDescription", "Commission Showcase");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
