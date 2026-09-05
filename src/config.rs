//! Config directory resolution.
//!
//! Resolved by hand rather than through `dirs`, whose macOS answer is
//! `~/Library/Application Support` — the README documents `~/.config` on both
//! Linux and macOS, so the two must not diverge.

use std::path::PathBuf;

/// The vademecum config directory, or `None` when the environment says nothing
/// useful (no `HOME` on Unix, no `%APPDATA%` on Windows).
pub fn config_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from).filter(|value| !value.as_os_str().is_empty());

    if cfg!(windows) { from_appdata(var("APPDATA")) } else { from_xdg(var("XDG_CONFIG_HOME"), var("HOME")) }
}

/// Unix: `$XDG_CONFIG_HOME/vademecum`, else `$HOME/.config/vademecum`.
fn from_xdg(xdg_config_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    match xdg_config_home {
        Some(xdg) => Some(xdg.join("vademecum")),
        None => home.map(|home| home.join(".config").join("vademecum")),
    }
}

/// Windows: `%APPDATA%\vademecum`.
fn from_appdata(appdata: Option<PathBuf>) -> Option<PathBuf> {
    appdata.map(|appdata| appdata.join("vademecum"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> Option<PathBuf> {
        Some(PathBuf::from(value))
    }

    #[test]
    fn xdg_config_home_wins_over_home() {
        assert_eq!(from_xdg(path("/xdg"), path("/home/otavio")), path("/xdg/vademecum"));
    }

    #[test]
    fn home_provides_the_dot_config_default() {
        assert_eq!(from_xdg(None, path("/home/otavio")), path("/home/otavio/.config/vademecum"));
    }

    #[test]
    fn nothing_in_the_environment_yields_nothing() {
        assert_eq!(from_xdg(None, None), None);
        assert_eq!(from_appdata(None), None);
    }

    #[test]
    fn appdata_is_the_windows_root() {
        let appdata = PathBuf::from(r"C:\Users\otavio\AppData\Roaming");
        assert_eq!(from_appdata(Some(appdata.clone())), Some(appdata.join("vademecum")));
    }
}
