// What differs per operating system, as data. Code that decides something per system takes a
// `&Platform`, so another system's rules are tested on this one.
#[derive(Debug)]
pub struct Platform {
    // Mojang's names: "windows", "linux", "osx"; and "x86_64" or "arm64".
    pub os: &'static str,
    pub arch: &'static str,
}

pub const WINDOWS: Platform = Platform { os: "windows", arch: "x86_64" };
pub const LINUX: Platform = Platform { os: "linux", arch: "x86_64" };

pub fn current() -> &'static Platform {
    if cfg!(windows) { &WINDOWS } else { &LINUX }
}
