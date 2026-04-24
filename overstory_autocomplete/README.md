# overstory_autocomplete

Reusable type-ahead autocomplete surface integration for Overstory.

This crate composes:
- `overstory::TextInput`
- `overstory::Dropdown`
- `overstory_list::ListViewController`

into a reusable autocomplete field controller with:
- anchored popup positioning
- debounced query refresh requests
- list-based option presentation
- keyboard and pointer selection handling
