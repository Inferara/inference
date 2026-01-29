//! TUI menu system.
//!
//! This module provides the main menu for the TUI application,
//! supporting keyboard navigation and shortcut keys.

use super::state::Screen;

/// A menu item with a label, shortcut key, and target screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItem {
    /// Display label for the menu item.
    pub label: &'static str,
    /// Single-character shortcut key.
    pub key: char,
    /// Target screen when this item is selected.
    pub screen: Option<Screen>,
    /// Whether this item quits the application.
    pub quits: bool,
}

impl MenuItem {
    /// Creates a menu item that navigates to a screen.
    const fn screen(label: &'static str, key: char, screen: Screen) -> Self {
        Self {
            label,
            key,
            screen: Some(screen),
            quits: false,
        }
    }

    /// Creates a menu item that quits the application.
    const fn quit(label: &'static str, key: char) -> Self {
        Self {
            label,
            key,
            screen: None,
            quits: true,
        }
    }
}

/// Static menu items for the main screen.
pub const MENU_ITEMS: &[MenuItem] = &[
    MenuItem::screen("Toolchains", 't', Screen::Toolchains),
    MenuItem::screen("Doctor", 'd', Screen::Doctor),
    MenuItem::quit("Quit", 'q'),
];

/// Menu state for keyboard navigation.
#[derive(Debug, Clone)]
pub struct Menu {
    /// Currently selected index.
    selected: usize,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    /// Creates a new menu with the first item selected.
    #[must_use]
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    /// Returns the currently selected index.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns the currently selected menu item.
    #[must_use]
    pub fn selected_item(&self) -> &MenuItem {
        &MENU_ITEMS[self.selected]
    }

    /// Moves selection up (wraps around).
    pub fn up(&mut self) {
        if self.selected == 0 {
            self.selected = MENU_ITEMS.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    /// Moves selection down (wraps around).
    pub fn down(&mut self) {
        self.selected = (self.selected + 1) % MENU_ITEMS.len();
    }

    /// Finds a menu item by its shortcut key.
    #[must_use]
    pub fn find_by_key(key: char) -> Option<&'static MenuItem> {
        MENU_ITEMS.iter().find(|item| item.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_default_selects_first_item() {
        let menu = Menu::default();
        assert_eq!(menu.selected(), 0);
    }

    #[test]
    fn menu_up_wraps_from_zero() {
        let mut menu = Menu::new();
        menu.up();
        assert_eq!(menu.selected(), MENU_ITEMS.len() - 1);
    }

    #[test]
    fn menu_down_wraps_at_end() {
        let mut menu = Menu::new();
        for _ in 0..MENU_ITEMS.len() {
            menu.down();
        }
        assert_eq!(menu.selected(), 0);
    }

    #[test]
    fn menu_up_and_down_cycle() {
        let mut menu = Menu::new();
        menu.down();
        assert_eq!(menu.selected(), 1);
        menu.up();
        assert_eq!(menu.selected(), 0);
    }

    #[test]
    fn find_by_key_returns_correct_item() {
        let item = Menu::find_by_key('t');
        assert!(item.is_some());
        assert_eq!(item.unwrap().screen, Some(Screen::Toolchains));

        let item = Menu::find_by_key('d');
        assert!(item.is_some());
        assert_eq!(item.unwrap().screen, Some(Screen::Doctor));

        let item = Menu::find_by_key('q');
        assert!(item.is_some());
        assert!(item.unwrap().quits);
    }

    #[test]
    fn find_by_key_returns_none_for_unknown() {
        let item = Menu::find_by_key('z');
        assert!(item.is_none());
    }

    #[test]
    fn menu_items_have_unique_keys() {
        let keys: Vec<char> = MENU_ITEMS.iter().map(|i| i.key).collect();
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn selected_item_returns_correct_item() {
        let mut menu = Menu::new();
        assert_eq!(menu.selected_item().key, 't');
        menu.down();
        assert_eq!(menu.selected_item().key, 'd');
    }
}
