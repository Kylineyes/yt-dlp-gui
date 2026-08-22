/// 用户选择的主题模式；`System` 会根据 Windows 当前主题解析有效主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// 新用户和未配置用户使用跟随系统模式。
    pub const DEFAULT: Self = Self::System;

    /// 返回用于配置持久化和跨模块传递的稳定字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// 将外部配置值解析为主题模式；未知值安全回退到跟随系统。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    /// 将用户选择、系统主题和深色能力合并为当前实际渲染主题。
    pub const fn resolve(self, system_theme: Option<EffectiveTheme>, dark_available: bool) -> EffectiveTheme {
        match self {
            Self::Light => EffectiveTheme::Light,
            Self::Dark if dark_available => EffectiveTheme::Dark,
            Self::Dark => EffectiveTheme::Light,
            Self::System => match (system_theme, dark_available) {
                (Some(EffectiveTheme::Dark), true) => EffectiveTheme::Dark,
                _ => EffectiveTheme::Light,
            },
        }
    }
}

/// 已解析的实际渲染主题，不代表用户的持久化选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveTheme {
    Light,
    Dark,
}

impl EffectiveTheme {
    /// 返回用于向 Slint 或配置边界传递的稳定字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// 用户可选的文本整体缩放档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextScale {
    Default,
    Large,
    ExtraLarge,
}

impl TextScale {
    /// 返回用于配置持久化的稳定字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Large => "large",
            Self::ExtraLarge => "extra-large",
        }
    }

    /// 将外部配置值解析为文本缩放档位；未知值回退默认字号。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "large" => Self::Large,
            "extra-large" | "extralarge" => Self::ExtraLarge,
            _ => Self::Default,
        }
    }

    /// 返回当前档位相对于默认字号的缩放倍率。
    pub const fn factor(self) -> f32 {
        match self {
            Self::Default => 1.0,
            Self::Large => 1.15,
            Self::ExtraLarge => 1.30,
        }
    }
}

/// 读取 Windows 当前应用主题；注册表不可用时返回 `None`。
#[cfg(windows)]
pub fn system_theme() -> Option<EffectiveTheme> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE,
    };

    let path: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut key: HKEY = null_mut();

    let result = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) };
    if result != ERROR_SUCCESS {
        return None;
    }

    let mut kind: REG_VALUE_TYPE = 0;
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegQueryValueExW(
            key,
            value.as_ptr(),
            null_mut(),
            &mut kind,
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    unsafe { RegCloseKey(key) };

    if result == ERROR_SUCCESS && size >= std::mem::size_of::<u32>() as u32 {
        Some(if data == 0 {
            EffectiveTheme::Dark
        } else {
            EffectiveTheme::Light
        })
    } else {
        None
    }
}

/// 非 Windows 平台没有项目定义的系统主题读取实现，使用浅色安全回退。
#[cfg(not(windows))]
pub const fn system_theme() -> Option<EffectiveTheme> {
    None
}

/// 当前版本的渲染令牌支持浅色和深色两套映射。
pub const fn dark_theme_available() -> bool {
    true
}
