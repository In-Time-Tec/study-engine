use std::path::PathBuf;

/// Resolve the user's home directory in a cross-platform way.
///
/// Unix sets `HOME`; Windows sets `USERPROFILE` (and `HOME` is usually
/// absent). Falling back to `HOME` only — as earlier code did — left
/// Windows users with a relative `.\.config/...` path that never resolved,
/// surfacing as "os error 3" (ERROR_PATH_NOT_FOUND) when loading a bank.
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mutates process-global env; serialized to avoid racing other env tests.
    #[test]
    fn home_dir_falls_back_to_userprofile_when_home_unset() {
        let prev_home = std::env::var_os("HOME");
        let prev_profile = std::env::var_os("USERPROFILE");
        unsafe {
            std::env::remove_var("HOME");
            std::env::set_var("USERPROFILE", "C:\\Users\\test");
        }

        let resolved = home_dir();

        unsafe {
            match prev_home {
                Some(h) => std::env::set_var("HOME", h),
                None => std::env::remove_var("HOME"),
            }
            match prev_profile {
                Some(p) => std::env::set_var("USERPROFILE", p),
                None => std::env::remove_var("USERPROFILE"),
            }
        }

        assert_eq!(resolved, PathBuf::from("C:\\Users\\test"));
    }
}
