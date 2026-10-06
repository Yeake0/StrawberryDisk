//! Resolve user intent before placing the window into collision-free shell gaps.
use crate::resident::preference_schema::TaskbarPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Edge {
    Left,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Windows10,
    Windows11Centered,
    Windows11LeftAligned,
    Unknown,
}

pub fn resolve(requested: TaskbarPosition, environment: Environment) -> Edge {
    match requested {
        TaskbarPosition::Left => Edge::Left,
        TaskbarPosition::Right => Edge::Right,
        TaskbarPosition::Auto => match environment {
            Environment::Windows11Centered => Edge::Left,
            _ => Edge::Right,
        },
    }
}

#[cfg(windows)]
pub fn read_environment() -> Environment {
    use std::ptr;
    use windows_sys::{
        core::w,
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
            System::Registry::{
                RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD,
                RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY,
            },
        },
    };
    // Fixed-size, read-only registry queries run with the existing shell inspection.
    // Re-read instead of caching failures so access recovery and alignment changes
    // take effect without restarting StrawberryDisk or modifying Explorer settings.
    unsafe {
        let mut build_text = [0u16; 32];
        let mut bytes = std::mem::size_of_val(&build_text) as u32;
        let result = RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            ptr::null_mut(),
            build_text.as_mut_ptr().cast(),
            &mut bytes,
        );
        if result != ERROR_SUCCESS {
            return Environment::Unknown;
        }
        let end = build_text
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(build_text.len());
        let build = String::from_utf16(&build_text[..end])
            .ok()
            .and_then(|text| text.parse::<u32>().ok());
        match build {
            Some(10_240..22_000) => return Environment::Windows10,
            Some(22_000..) => {}
            _ => return Environment::Unknown,
        }
        let mut alignment = 0u32;
        let mut bytes = std::mem::size_of_val(&alignment) as u32;
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced"),
            w!("TaskbarAl"),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            (&mut alignment as *mut u32).cast(),
            &mut bytes,
        );
        match (result, alignment) {
            (ERROR_SUCCESS, 0) => Environment::Windows11LeftAligned,
            (ERROR_SUCCESS, 1) => Environment::Windows11Centered,
            // An unset alignment uses Windows 11's centered default. Other errors
            // and unrecognized values remain unknown and resolve conservatively.
            (ERROR_FILE_NOT_FOUND, _) => Environment::Windows11Centered,
            _ => Environment::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_position_follows_environment_and_manual_choices_stay_fixed() {
        for (environment, expected) in [
            (Environment::Windows10, Edge::Right),
            (Environment::Windows11Centered, Edge::Left),
            (Environment::Windows11LeftAligned, Edge::Right),
            (Environment::Unknown, Edge::Right),
        ] {
            assert_eq!(resolve(TaskbarPosition::Auto, environment), expected);
            assert_eq!(resolve(TaskbarPosition::Left, environment), Edge::Left);
            assert_eq!(resolve(TaskbarPosition::Right, environment), Edge::Right);
        }
    }
}
