fn main() {
    println!("cargo::rustc-check-cfg=cfg(nightly)");

    let version = rustc_version::version_meta().unwrap();
    if version.channel == rustc_version::Channel::Nightly {
        println!("cargo:rustc-cfg=nightly");
    }
}
