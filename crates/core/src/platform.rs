// What differs per operating system, as data. Code that decides something per system takes a
// `&Platform`, so another system's rules are tested on this one.
#[derive(Debug)]
pub struct Platform {
    // Mojang's names: "windows", "linux", "osx"; and "x86_64" or "arm64".
    pub os: &'static str,
    pub arch: &'static str,
    // The key in Mojang's list of Java runtimes.
    pub java_runtime: &'static str,
    // Where `bin` is inside a downloaded runtime: on macOS it comes as an app bundle.
    pub java_home: &'static str,
    // In the runtime's `bin`: the one that starts the game, and the one that prints to a console.
    pub java: &'static str,
    pub console_java: &'static str,
    pub classpath_separator: &'static str,
}

pub const WINDOWS: Platform = Platform { os: "windows", arch: "x86_64", java_runtime: "windows-x64", java_home: "", java: "javaw.exe", console_java: "java.exe", classpath_separator: ";" };
pub const LINUX: Platform = Platform { os: "linux", arch: "x86_64", java_runtime: "linux", java_home: "", java: "java", console_java: "java", classpath_separator: ":" };
// Apple chips only: there is no build for Intel Macs.
pub const MACOS: Platform = Platform { os: "osx", arch: "arm64", java_runtime: "mac-os-arm64", java_home: "jre.bundle/Contents/Home", java: "java", console_java: "java", classpath_separator: ":" };

pub fn current() -> &'static Platform {
    if cfg!(windows) {
        &WINDOWS
    } else if cfg!(target_os = "macos") {
        &MACOS
    } else {
        &LINUX
    }
}
