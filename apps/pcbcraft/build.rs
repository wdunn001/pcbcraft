#![forbid(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/brand/generated/windows/pcbcraft.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }
    if !icon.is_file() {
        println!(
            "cargo:warning=missing {}; Windows exe icon not embedded (run packaging/windows/make-icon.sh for release builds)",
            icon.display()
        );
        return Ok(());
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    let copy = out.join("pcbcraft.ico");
    std::fs::copy(&icon, &copy)?;
    let rc = out.join("pcbcraft.rc");
    std::fs::write(
        &rc,
        format!(
            "1 ICON \"{}\"\n",
            copy.display().to_string().replace('\\', "/")
        ),
    )?;
    embed_resource::compile_for(&rc, ["pcbcraft"], embed_resource::NONE).manifest_required()?;
    Ok(())
}
