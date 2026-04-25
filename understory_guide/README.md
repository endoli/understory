# Understory Guide

`understory_guide` provides small, headless 2D guide geometry primitives.

It owns:

- line-guide pose and projection math
- semantic hit targets for guide body and handles
- lifting `understory_axis::AxisRuler1D` marks onto a 2D guide

It does not own:

- rendering
- text shaping
- event routing
- domain navigation policy

Typical usage:

1. derive an `AxisRuler1D` from `understory_axis`
2. place it on screen with a `LineGuide2D`
3. render the resulting `AxisGuide2D` marks in app code
