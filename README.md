# zellij-nucleo

This crate provides a fuzzy finder widget based on
[nucleo-matcher](https://crates.io/crates/nucleo-matcher) for use in
[zellij plugins](https://zellij.dev/documentation/plugins). It can be used by
your own plugins to allow easy searching through a list of options, and
automatically handles the picker UI as needed.

## Usage

A basic plugin that uses the `zellij-nucleo` crate to switch tabs can be
structured like this:

```rust
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    picker: zellij_nucleo::Picker<u32>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(
        &mut self,
        configuration: std::collections::BTreeMap<String, String>,
    ) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);

        subscribe(&[EventType::TabUpdate]);
        self.picker.load(&configuration);
    }

    fn update(&mut self, event: Event) -> bool {
        match self.picker.update(&event) {
            Some(zellij_nucleo::Response::Select(entry)) => {
                go_to_tab(entry.data);
                close_self();
            }
            Some(zellij_nucleo::Response::Cancel) => {
                close_self();
            }
            None => {}
        }

        if let Event::TabUpdate(tabs) = event {
            self.picker.clear();
            self.picker.extend(tabs.iter().map(|tab| zellij_nucleo::Entry {
                data: u32::try_from(tab.position).unwrap(),
                string: format!("{}: {}", tab.position + 1, tab.name),
            }));
        }

        self.picker.needs_redraw()
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.picker.render(rows, cols);
    }
}
```

## Notes

Unlike the (nucleo)[https://crates.io/crates/nucleo] crate, this can't run
concurrently due to existing limitations with the zellij plugin interface.
This shouldn't matter for reasonably short lists, but may be noticeable for
larger lists.
