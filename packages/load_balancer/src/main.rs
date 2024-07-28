#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(not(target_os = "windows"))]
mod server;

fn main() {
    #[cfg(not(target_os = "windows"))]
    {
        server::serve()
    }
    #[cfg(target_os = "windows")]
    {
        panic!("Windows OS is not supported. Blame the way daemons are designed.")
    }
}
