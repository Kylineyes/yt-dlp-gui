#[cfg(windows)]
pub fn system_is_dark() -> bool {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_VALUE_TYPE, RegCloseKey, RegOpenKeyExW,
        RegQueryValueExW,
    };
    use windows::core::PCWSTR;

    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut key = HKEY_CURRENT_USER;
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            Some(0),
            KEY_READ,
            &mut key,
        )
    };
    if opened.is_err() {
        return false;
    }

    let mut value_type = REG_VALUE_TYPE(0);
    let mut value = 0u32;
    let mut value_size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            Some((&mut value as *mut u32).cast()),
            Some(&mut value_size),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result.is_ok() && value_type == REG_DWORD && value == 0
}

#[cfg(not(windows))]
pub fn system_is_dark() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_windows_fallback_is_light() {
        #[cfg(not(windows))]
        assert!(!super::system_is_dark());
    }
}
