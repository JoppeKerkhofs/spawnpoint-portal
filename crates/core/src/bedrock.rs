// Minecraft Bedrock is installed and updated by the Microsoft Store. Portal only finds and starts it.
use std::process::Command;

use crate::{Error, Result};

#[cfg(windows)]
const PACKAGES: &str = r"HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
const PACKAGE: &str = "Microsoft.MinecraftUWP_";

// The version in a line such as ...\Packages\Microsoft.MinecraftUWP_1.21.12301.0_x64__8wekyb3d8bbwe.
pub fn version_in(listing: &str) -> Option<String> {
    let version = listing.split(PACKAGE).nth(1)?.split('_').next()?;
    let plain = !version.is_empty() && version.len() <= 40 && version.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    plain.then(|| version.to_string())
}

// The installed version, or None when Bedrock is not installed for this Windows user.
#[cfg(windows)]
pub fn installed() -> Option<String> {
    use std::os::windows::process::CommandExt;
    let found = Command::new("reg").args(["query", PACKAGES, "/k", "/f", PACKAGE]).creation_flags(0x0800_0000).output().ok()?;
    version_in(&String::from_utf8_lossy(&found.stdout))
}

#[cfg(not(windows))]
pub fn installed() -> Option<String> {
    None
}

// Windows starts the game itself through the link type Bedrock registers.
pub fn start() -> Result<()> {
    Command::new("explorer.exe").arg("minecraft://").spawn().map(drop).map_err(|e| Error::Other(format!("Minecraft Bedrock could not be started: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_read_from_the_package_name() {
        let listing = "\r\nHKEY_CURRENT_USER\\Software\\...\\Packages\\Microsoft.MinecraftUWP_1.21.12301.0_x64__8wekyb3d8bbwe\r\nEnd of search: 1 match(es) found.\r\n";
        assert_eq!(version_in(listing).as_deref(), Some("1.21.12301.0"));
        assert_eq!(version_in("End of search: 0 match(es) found."), None);
        assert_eq!(version_in("Microsoft.MinecraftUWP_<b>_x64"), None);
        assert_eq!(version_in("Microsoft.MinecraftUWP__x64"), None);
    }
}
