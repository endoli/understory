# Understory Axis

Headless numeric axis mapping and tick primitives for Understory.

This crate owns:

- 1D linear and log axis mappings
- configurable major-step ladders
- configurable subdivision policy
- major / medium / minor tick classification
- labeled-tick eligibility
- spacing metadata for callers that want consistent axis-derived policy

Typical usage:

- define an `AxisMapping1D` for the visible domain and view span
- derive an `AxisScale1D` from that mapping
- iterate ticks across a visible numeric range
- format labels in the caller's own domain language

It does not own domain-specific label formatting such as timestamps, dates, or
units, and it does not own chart layout or rendering.
