#![allow(dead_code)]

use super::contracts::Route;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationState {
    current: Route,
}

impl NavigationState {
    pub const fn new() -> Self {
        Self {
            current: Route::Welcome,
        }
    }

    pub const fn current(self) -> Route {
        self.current
    }

    pub fn navigate_to(&mut self, route: Route) {
        self.current = route;
    }

    pub fn navigate_to_index(&mut self, index: i32) -> bool {
        let Some(route) = Route::from_index(index) else {
            return false;
        };

        self.navigate_to(route);
        true
    }
}

impl Default for NavigationState {
    fn default() -> Self {
        Self::new()
    }
}
