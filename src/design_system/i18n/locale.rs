/// 应用支持的界面语言；字符串形式使用 BCP-47 locale 标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    ZhCn,
    EnUs,
}

impl Locale {
    /// 未配置语言或系统语言不受支持时使用英文。
    pub const DEFAULT: Self = Self::EnUs;

    /// 返回用于配置和 Slint global 的稳定 locale 标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }

    /// 识别当前 Windows 用户首选显示语言；非简体中文统一回退到英文。
    pub fn system() -> Self {
        #[cfg(windows)]
        {
            use std::ptr::null_mut;

            use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

            let mut language_count = 0;
            let mut buffer_length = 0;
            let success = unsafe {
                GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut language_count, null_mut(), &mut buffer_length)
            };
            if success == 0 || buffer_length == 0 {
                return Self::DEFAULT;
            }

            let mut buffer = vec![0u16; buffer_length as usize];
            let success = unsafe {
                GetUserPreferredUILanguages(
                    MUI_LANGUAGE_NAME,
                    &mut language_count,
                    buffer.as_mut_ptr(),
                    &mut buffer_length,
                )
            };
            if success == 0 {
                return Self::DEFAULT;
            }

            let Some(language) = buffer
                .split(|character| *character == 0)
                .find(|language| !language.is_empty())
            else {
                return Self::DEFAULT;
            };
            return Self::parse(&String::from_utf16_lossy(language));
        }

        #[cfg(not(windows))]
        {
            Self::DEFAULT
        }
    }

    /// 解析 BCP-47 语言标识；未知语言统一回退默认语言。
    pub fn parse(value: &str) -> Self {
        let normalized = value.trim().to_ascii_lowercase();
        let mut components = normalized.split('-');
        match (components.next(), components.next()) {
            (Some("en"), _) => Self::EnUs,
            (Some("zh"), None | Some("hans" | "cn" | "sg" | "my")) => Self::ZhCn,
            _ => Self::DEFAULT,
        }
    }
}
