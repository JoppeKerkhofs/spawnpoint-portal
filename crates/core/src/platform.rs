// What differs per operating system, as data. Code that decides something per system takes a
// `&Platform`, so another system's rules are tested on this one.
#[derive(Debug)]
pub struct Platform {
    // Mojang's names: "windows", "linux", "osx"; and "x86_64" or "arm64".
    pub os: &'static str,
    pub arch: &'static str,
    // The key in Mojang's list of Java runtimes.
    pub java_runtime: &'static str,
    // In the runtime's `bin`: the one that starts the game, and the one that prints to a console.
    pub java: &'static str,
    pub console_java: &'static str,
    pub classpath_separator: &'static str,
}

pub const WINDOWS: Platform = Platform { os: "windows", arch: "x86_64", java_runtime: "windows-x64", java: "javaw.exe", console_java: "java.exe", classpath_separator: ";" };
pub const LINUX: Platform = Platform { os: "linux", arch: "x86_64", java_runtime: "linux", java: "java", console_java: "java", classpath_separator: ":" };

pub fn current() -> &'static Platform {
    if cfg!(windows) { &WINDOWS } else { &LINUX }
}
