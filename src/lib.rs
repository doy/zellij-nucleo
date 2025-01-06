//! This crate provides a fuzzy finder widget based on
//! [nucleo-matcher](https://crates.io/crates/nucleo-matcher) for use in
//! [zellij plugins](https://zellij.dev/documentation/plugins). It can be used by
//! your own plugins to allow easy searching through a list of options, and
//! automatically handles the picker UI as needed.
//!
//! ## Usage
//!
//! A basic plugin that uses the `zellij-nucleo` crate to switch tabs can be
//! structured like this:
//!
//! ```rust
//! use zellij_tile::prelude::*;
//!
//! #[derive(Default)]
//! struct State {
//!     picker: zellij_nucleo::Picker<u32>,
//! }
//!
//! register_plugin!(State);
//!
//! impl ZellijPlugin for State {
//!     fn load(
//!         &mut self,
//!         configuration: std::collections::BTreeMap<String, String>,
//!     ) {
//!         request_permission(&[
//!             PermissionType::ReadApplicationState,
//!             PermissionType::ChangeApplicationState,
//!         ]);
//!
//!         subscribe(&[EventType::TabUpdate]);
//!         self.picker.load(&configuration);
//!     }
//!
//!     fn update(&mut self, event: Event) -> bool {
//!         match self.picker.update(&event) {
//!             Some(zellij_nucleo::Response::Select(entry)) => {
//!                 go_to_tab(entry.data);
//!                 close_self();
//!             }
//!             Some(zellij_nucleo::Response::Cancel) => {
//!                 close_self();
//!             }
//!             None => {}
//!         }
//!
//!         if let Event::TabUpdate(tabs) = event {
//!             self.picker.clear();
//!             self.picker.extend(tabs.iter().map(|tab| zellij_nucleo::Entry {
//!                 data: u32::try_from(tab.position).unwrap(),
//!                 string: format!("{}: {}", tab.position + 1, tab.name),
//!             }));
//!         }
//!
//!         self.picker.needs_redraw()
//!     }
//!
//!     fn render(&mut self, rows: usize, cols: usize) {
//!         self.picker.render(rows, cols);
//!     }
//! }
//! ```

use zellij_tile::prelude::*;

use std::fmt::Write as _;

use owo_colors::OwoColorize as _;
use unicode_width::UnicodeWidthChar as _;

const PICKER_EVENTS: &[EventType] = &[EventType::Key];

/// An entry in the picker.
///
/// The type parameter corresponds to the type of the additional data
/// associated with each entry.
#[derive(Debug, Clone, Default)]
pub struct Entry<T> {
    /// String that will be displayed in the picker window, and filtered when
    /// searching.
    pub string: String,
    /// Extra data associated with the picker entry, which can be retrieved
    /// when an entry is selected.
    pub data: T,
}

impl<T> AsRef<str> for Entry<T> {
    fn as_ref(&self) -> &str {
        &self.string
    }
}

/// Possible results from the picker.
#[derive(Debug)]
pub enum Response<T> {
    /// The user selected a specific entry.
    Select(Entry<T>),
    /// The user closed the picker without selecting an entry.
    Cancel,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    #[default]
    Normal,
    Search,
}

/// State of the picker itself.
#[derive(Default)]
pub struct Picker<T: Clone> {
    query: String,
    all_entries: Vec<Entry<T>>,
    search_results: Vec<SearchResult<T>>,
    selected: usize,
    input_mode: InputMode,
    needs_redraw: bool,

    pattern: nucleo_matcher::pattern::Pattern,
    matcher: nucleo_matcher::Matcher,
    case_matching: nucleo_matcher::pattern::CaseMatching,
}

impl<T: Clone> Picker<T> {
    /// This function must be called during your plugin's
    /// [`load`](zellij_tile::ZellijPlugin::load) function.
    pub fn load(
        &mut self,
        configuration: &std::collections::BTreeMap<String, String>,
    ) {
        subscribe(PICKER_EVENTS);

        match configuration
            .get("nucleo_case_matching")
            .map(|s| s.as_ref())
        {
            Some("respect") => {
                self.case_matching =
                    nucleo_matcher::pattern::CaseMatching::Respect
            }
            Some("ignore") => {
                self.case_matching =
                    nucleo_matcher::pattern::CaseMatching::Ignore
            }
            Some("smart") => {
                self.case_matching =
                    nucleo_matcher::pattern::CaseMatching::Smart
            }
            Some(s) => {
                panic!("unrecognized value {s} for option 'nucleo_case_matching': expected 'respect', 'ignore', 'smart'");
            }
            None => {}
        }

        match configuration.get("nucleo_match_paths").map(|s| s.as_ref()) {
            Some("true") => {
                self.set_match_paths();
            }
            Some("false") => {
                self.clear_match_paths();
            }
            Some(s) => {
                panic!("unrecognized value {s} for option 'nucleo_match_paths': expected 'true', 'false'");
            }
            None => {}
        }

        match configuration
            .get("nucleo_start_in_search_mode")
            .map(|s| s.as_ref())
        {
            Some("true") => {
                self.enter_search_mode();
            }
            Some("false") => {
                self.enter_normal_mode();
            }
            Some(s) => {
                panic!("unrecognized value {s} for option 'nucleo_start_in_search_mode': expected 'true', 'false'");
            }
            None => {}
        }
    }

    /// This function must be called during your plugin's
    /// [`update`](zellij_tile::ZellijPlugin::update) function. If an entry
    /// was selected or the picker was closed, this function will return a
    /// [`Response`]. This function will update the picker's internal state
    /// of whether it needs to redraw the picker, so your plugin's
    /// [`update`](zellij_tile::ZellijPlugin::update) function should return
    /// true if [`needs_redraw`](Self::needs_redraw) returns true.
    pub fn update(&mut self, event: &Event) -> Option<Response<T>> {
        match event {
            Event::Key(key) => self.handle_key(key),
            _ => None,
        }
    }

    /// This function must be called during your plugin's
    /// [`render`](zellij_tile::ZellijPlugin::render) function.
    pub fn render(&mut self, rows: usize, cols: usize) {
        if rows == 0 {
            return;
        }

        let visible_entry_count = rows - 1;
        let visible_entries: Vec<SearchResult<T>> = self
            .search_results
            .iter()
            .skip((self.selected / visible_entry_count) * visible_entry_count)
            .take(visible_entry_count)
            .cloned()
            .collect();
        let visible_selected = self.selected % visible_entry_count;

        print!("  ");
        if self.input_mode == InputMode::Normal && self.query.is_empty() {
            print!(
                "{}",
                "(press / to search)".fg::<owo_colors::colors::BrightBlack>()
            );
        } else {
            print!("{}", self.query);
            if self.input_mode == InputMode::Search {
                print!("{}", " ".bg::<owo_colors::colors::Green>());
            }
        }
        println!();

        let lines: Vec<_> = visible_entries
            .iter()
            .enumerate()
            .map(|(i, search_result)| {
                let mut line = String::new();

                if i == visible_selected {
                    write!(
                        &mut line,
                        "{} ",
                        ">".fg::<owo_colors::colors::Yellow>()
                    )
                    .unwrap();
                } else {
                    write!(&mut line, "  ").unwrap();
                }

                let mut current_col = 2;
                for (char_idx, c) in
                    search_result.entry.string.chars().enumerate()
                {
                    let width = c.width().unwrap_or(0);
                    if current_col + width > cols - 6 {
                        write!(
                            &mut line,
                            "{}",
                            " [...]".fg::<owo_colors::colors::BrightBlack>()
                        )
                        .unwrap();
                        break;
                    }

                    if search_result
                        .indices
                        .contains(&u32::try_from(char_idx).unwrap())
                    {
                        write!(
                            &mut line,
                            "{}",
                            c.fg::<owo_colors::colors::Cyan>()
                        )
                        .unwrap();
                    } else if i == visible_selected {
                        write!(
                            &mut line,
                            "{}",
                            c.fg::<owo_colors::colors::Yellow>()
                        )
                        .unwrap();
                    } else {
                        write!(&mut line, "{}", c).unwrap();
                    }

                    current_col += width;
                }
                line
            })
            .collect();

        print!("{}", lines.join("\n"));

        self.needs_redraw = false;
    }

    /// Returns true if the picker needs to be redrawn. Your plugin's
    /// [`update`](zellij_tile::ZellijPlugin::update) function should return
    /// true if this function returns true.
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Returns the current list of entries in the picker.
    pub fn entries(&self) -> &[Entry<T>] {
        &self.all_entries
    }

    /// Forces a specific entry in the list of entries to be selected.
    pub fn select(&mut self, idx: usize) {
        self.selected = idx;
        self.needs_redraw = true;
    }

    /// Removes all entries in the list.
    pub fn clear(&mut self) {
        self.all_entries.clear();
        self.search();
    }

    /// Adds new entries to the list.
    pub fn extend(&mut self, iter: impl IntoIterator<Item = Entry<T>>) {
        self.all_entries.extend(iter);
        self.search();
    }

    /// Request that the fuzzy matcher always respect case when matching.
    pub fn use_case_matching_respect(&mut self) {
        self.case_matching = nucleo_matcher::pattern::CaseMatching::Respect;
    }

    /// Request that the fuzzy matcher always ignore case when matching.
    pub fn use_case_matching_ignore(&mut self) {
        self.case_matching = nucleo_matcher::pattern::CaseMatching::Ignore;
    }

    /// Request that the fuzzy matcher respect case when matching if the
    /// query contains any uppercase characters, but ignore case otherwise.
    /// This is the default.
    pub fn use_case_matching_smart(&mut self) {
        self.case_matching = nucleo_matcher::pattern::CaseMatching::Smart;
    }

    /// Puts the picker into search mode (equivalent to pressing `/` when in
    /// normal mode).
    pub fn enter_search_mode(&mut self) {
        self.input_mode = InputMode::Search;
    }

    /// Puts the picker into normal mode (equivalent to pressing Escape when
    /// in search mode). This is the default.
    pub fn enter_normal_mode(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    /// Configures the fuzzy matcher to adjust matching bonuses appropriate
    /// for matching paths.
    pub fn set_match_paths(&mut self) {
        self.matcher.config.set_match_paths();
    }

    /// Configures the fuzzy matcher to adjust matching bonuses appropriate
    /// for matching arbitrary strings. This is the default.
    pub fn clear_match_paths(&mut self) {
        self.matcher.config = nucleo_matcher::Config::DEFAULT;
    }

    fn search(&mut self) {
        let prev_selected = self
            .search_results
            .get(self.selected)
            .map(|search_result| search_result.entry.clone());

        self.pattern.reparse(
            &self.query,
            self.case_matching,
            nucleo_matcher::pattern::Normalization::Smart,
        );
        let mut haystack = vec![];
        self.search_results = self
            .all_entries
            .iter()
            .filter_map(|entry| {
                let haystack = nucleo_matcher::Utf32Str::new(
                    &entry.string,
                    &mut haystack,
                );
                let mut indices = vec![];
                self.pattern
                    .indices(haystack, &mut self.matcher, &mut indices)
                    .map(|score| SearchResult {
                        entry: entry.clone(),
                        score,
                        indices,
                    })
            })
            .collect();
        self.search_results.sort();

        if let Some(prev_selected) = prev_selected {
            self.selected = self
                .search_results
                .iter()
                .enumerate()
                .find_map(|(idx, search_result)| {
                    (search_result.entry.string == prev_selected.string)
                        .then_some(idx)
                })
                .unwrap_or(0);
        }

        self.needs_redraw = true;
    }

    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<Response<T>> {
        self.handle_global_key(key)
            .or_else(|| match self.input_mode {
                InputMode::Normal => self.handle_normal_key(key),
                InputMode::Search => self.handle_search_key(key),
            })
    }

    fn handle_normal_key(
        &mut self,
        key: &KeyWithModifier,
    ) -> Option<Response<T>> {
        match key.bare_key {
            BareKey::Char('j') if key.has_no_modifiers() => {
                self.down();
            }
            BareKey::Char('k') if key.has_no_modifiers() => {
                self.up();
            }
            BareKey::Char(c @ '1'..='8') if key.has_no_modifiers() => {
                let position =
                    usize::try_from(c.to_digit(10).unwrap() - 1).unwrap();
                return self.search_results.get(position).map(
                    |search_result| {
                        Response::Select(search_result.entry.clone())
                    },
                );
            }
            BareKey::Char('9') if key.has_no_modifiers() => {
                return self.search_results.last().map(|search_result| {
                    Response::Select(search_result.entry.clone())
                })
            }
            BareKey::Char('/') if key.has_no_modifiers() => {
                self.input_mode = InputMode::Search;
                self.needs_redraw = true;
            }
            _ => {}
        }

        None
    }

    fn handle_search_key(
        &mut self,
        key: &KeyWithModifier,
    ) -> Option<Response<T>> {
        match key.bare_key {
            BareKey::Char(c) if key.has_no_modifiers() => {
                self.query.push(c);
                self.search();
                self.selected = 0;
            }
            BareKey::Char('u') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.query.clear();
                self.search();
                self.selected = 0;
            }
            BareKey::Backspace if key.has_no_modifiers() => {
                self.query.pop();
                self.search();
                self.selected = 0;
            }
            _ => {}
        }

        None
    }

    fn handle_global_key(
        &mut self,
        key: &KeyWithModifier,
    ) -> Option<Response<T>> {
        match key.bare_key {
            BareKey::Tab if key.has_no_modifiers() => {
                self.down();
            }
            BareKey::Down if key.has_no_modifiers() => {
                self.down();
            }
            BareKey::Tab if key.has_modifiers(&[KeyModifier::Shift]) => {
                self.up();
            }
            BareKey::Up if key.has_no_modifiers() => {
                self.up();
            }
            BareKey::Esc if key.has_no_modifiers() => {
                self.input_mode = InputMode::Normal;
                self.needs_redraw = true;
            }
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                return Some(Response::Cancel);
            }
            BareKey::Enter if key.has_no_modifiers() => {
                return Some(Response::Select(
                    self.search_results[self.selected].entry.clone(),
                ));
            }
            _ => {}
        }

        None
    }

    fn down(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.selected = (self.search_results.len() + self.selected + 1)
            % self.search_results.len();
        self.needs_redraw = true;
    }

    fn up(&mut self) {
        if self.search_results.is_empty() {
            return;
        }
        self.selected = (self.search_results.len() + self.selected - 1)
            % self.search_results.len();
        self.needs_redraw = true;
    }
}

#[derive(Clone)]
struct SearchResult<T> {
    entry: Entry<T>,
    score: u32,
    indices: Vec<u32>,
}

impl<T> Ord for SearchResult<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score
            .cmp(&other.score)
            .reverse()
            .then_with(|| self.indices.first().cmp(&other.indices.first()))
            .then_with(|| self.entry.string.cmp(&other.entry.string))
    }
}

impl<T> PartialOrd for SearchResult<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Eq for SearchResult<T> {}

impl<T> PartialEq for SearchResult<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
