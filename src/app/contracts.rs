#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Welcome,
    Configure,
    Search,
    Tasks,
}

impl Route {
    pub const ALL: [Self; 4] = [Self::Welcome, Self::Configure, Self::Search, Self::Tasks];

    pub const fn index(self) -> i32 {
        match self {
            Self::Welcome => 0,
            Self::Configure => 1,
            Self::Search => 2,
            Self::Tasks => 3,
        }
    }

    pub const fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Welcome),
            1 => Some(Self::Configure),
            2 => Some(Self::Search),
            3 => Some(Self::Tasks),
            _ => None,
        }
    }
}

pub trait ThemeProvider {}

pub trait I18nProvider {}

pub trait Storage {}

pub trait DialogService {}
