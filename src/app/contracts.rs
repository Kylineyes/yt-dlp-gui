#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Welcome,
    Configure,
    Search,
    Tasks,
}

pub trait ThemeProvider {}

pub trait I18nProvider {}

pub trait Storage {}

pub trait DialogService {}
