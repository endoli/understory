# Understory Axis

Headless numeric axis scale and tick primitives for Understory.

This crate owns:

- 1D axis scale selection from world-units-per-pixel
- configurable major-step ladders
- configurable subdivision policy
- major / medium / minor tick classification
- labeled-tick eligibility
- spacing metadata for callers that want consistent axis-derived policy

Typical usage:

- derive an `AxisScale1D` from world-units-per-pixel
- iterate ticks across a visible numeric range
- format labels in the caller's own domain language

It does not own domain-specific label formatting such as timestamps, dates, or
units. It currently models linear numeric axes only; a true logarithmic axis is
a separate future concern.
