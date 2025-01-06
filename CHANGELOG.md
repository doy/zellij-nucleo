# Changelog

## [0.0.5] - 2025-01-05

### Changed

* Made the setting of configuration options more consistent - now all options
  can be set by individual plugins in code, and the configuration that is
  passed in via the zellij configuration will override those settings if they
  exist, and leave the plugin-specified defaults alone otherwise.

## [0.0.4] - 2025-01-05

### Added

* Added `nucleo_start_in_search_mode` option for configuring the mode to
  start in.

## [0.0.3] - 2025-01-05

### Added

* Added `^U` keybinding for clearing the input.
* Added `nucleo_case_matching` option for configuring case matching.
* Added `nucleo_match_paths` option for configuring scoring bonuses for path
  matching.

## [0.0.2] - 2025-01-05

### Added

* Added `entries` function to get the current list of entries in the picker.

### Changed

* Improved sorting for search results scored identically by nucleo.

## [0.0.1] - 2025-01-05

### Added

* Initial release
