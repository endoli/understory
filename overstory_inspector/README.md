# Overstory Inspector

Inspector and property-grid surfaces for Overstory.

This crate sits above `understory_inspector` and `overstory`. It owns the
small but repeated UI wiring for:
- projecting inspector rows into Overstory `TextBlock`s,
- mapping row clicks back into inspector keys,
- and rendering structured property-grid rows with swatches and badges.

It does not own domain models or property projection policy.
