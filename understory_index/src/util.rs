// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// Returns the square root of the number, rounded up.
#[inline]
pub(crate) const fn isqrt_ceil(num: usize) -> usize {
    let s = num.isqrt();

    // This multiplication cannot overflow because `s` is the rounded-down square root of `num`,
    // i.e., `s * s` is guaranteed to be less than or equal to `num`.
    if s * s < num { s + 1 } else { s }
}

#[cfg(test)]
mod tests {
    #[test]
    fn isqrt_ceil() {
        assert_eq!(super::isqrt_ceil(255), 16);
        assert_eq!(super::isqrt_ceil(256), 16);
        assert_eq!(super::isqrt_ceil(257), 17);
    }
}
