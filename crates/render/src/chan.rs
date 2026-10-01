//! Channel descriptors (SPEC.md §7): each descriptor byte is
//! `(code<<4) | nbits` with `channames = "rgbkamx"`; the first channel of
//! the string is the most significant descriptor byte.

use std::fmt;
use std::str::FromStr;

/// Channel code letters, indexed by code (`draw.h` channames).
const CHANNAMES: &[u8; 7] = b"rgbkamx";

/// A u32 holds four descriptor bytes.
const MAX_CHANNELS: usize = 4;
/// Single channels are 1..=8 bits wide in plan9.
const MAX_CBITS: u32 = 8;
/// A pixel word is one u32.
const MAX_DEPTH: u32 = 32;

/// Channel descriptor packed exactly as on the draw wire (SPEC.md §7, the
/// 'b' allocimage `chan[4]` field): bytes are `(code<<4) | nbits`, first
/// string channel in the high byte. `x8r8g8b8` = `0x68081828`, `GREY1` =
/// `0x31` (both confirmed by the 2026-10-01 live capture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chan(pub u32);

impl Chan {
    pub const GREY1: Chan = Chan(0x31);
    pub const GREY2: Chan = Chan(0x32);
    pub const GREY4: Chan = Chan(0x34);
    pub const GREY8: Chan = Chan(0x38);
    pub const CMAP8: Chan = Chan(0x58);
    pub const RGB15: Chan = Chan(0x61051525);
    pub const RGB16: Chan = Chan(0x051625);
    pub const RGB24: Chan = Chan(0x081828);
    pub const BGR24: Chan = Chan(0x281808);
    pub const RGBA32: Chan = Chan(0x08182848);
    pub const ARGB32: Chan = Chan(0x48081828);
    pub const ABGR32: Chan = Chan(0x48281808);
    /// Default X11/Linux screen channel (live capture 2026-10-01).
    pub const XRGB32: Chan = Chan(0x68081828);
    pub const XBGR32: Chan = Chan(0x68281808);

    /// Descriptor bytes from most significant to least significant.
    /// Short descriptors (e.g. `RGB24 = 0x00081828`) simply have leading
    /// zero bytes; `depth`/`Display` skip them.
    fn desc_bytes(self) -> impl Iterator<Item = u8> {
        let desc = self.0;
        (0..4).map(move |i| (desc >> (24 - 8 * i)) as u8)
    }

    /// Total depth in bits: sum of channel widths (`chantodepth`).
    pub fn depth(self) -> u32 {
        self.desc_bytes()
            .filter(|&b| b != 0)
            .map(|b| (b & 0x0f) as u32)
            .sum()
    }
}

/// `chantostr`: channel pairs from the highest nonzero byte down; e.g.
/// `0x68081828` → `"x8r8g8b8"`, `0x00081828` → `"r8g8b8"`.
impl fmt::Display for Chan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.desc_bytes() {
            if b == 0 {
                continue;
            }
            let code = (b >> 4) as usize;
            let nbits = b & 0x0f;
            if code >= CHANNAMES.len() || nbits == 0 {
                return write!(f, "<bad:{:#04x}>", b);
            }
            write!(f, "{}{}", CHANNAMES[code] as char, nbits)?;
        }
        Ok(())
    }
}

/// Parse a channel string: letter+digit pairs, e.g. `"x8r8g8b8"`, `"k1"`,
/// `"m8"`. The first pair becomes the most significant descriptor byte,
/// matching `chantostr` order.
impl FromStr for Chan {
    type Err = ChanError;

    fn from_str(s: &str) -> Result<Chan, ChanError> {
        let b = s.as_bytes();
        if b.is_empty() {
            return Err(ChanError::Empty);
        }
        if b.len() % 2 != 0 {
            return Err(ChanError::OddLength);
        }
        if b.len() / 2 > MAX_CHANNELS {
            return Err(ChanError::TooManyChannels);
        }
        let mut desc: u32 = 0;
        let mut depth: u32 = 0;
        let mut i = 0;
        while i < b.len() {
            let code = CHANNAMES
                .iter()
                .position(|&c| c == b[i])
                .ok_or(ChanError::BadChar(b[i] as char))?;
            let nbits = (b[i + 1] as char)
                .to_digit(10)
                .ok_or(ChanError::BadChar(b[i + 1] as char))?;
            if nbits == 0 || nbits > MAX_CBITS {
                return Err(ChanError::BadDepth(nbits));
            }
            depth += nbits;
            if depth > MAX_DEPTH {
                return Err(ChanError::TooDeep(depth));
            }
            desc = (desc << 8) | ((code as u32) << 4) | nbits;
            i += 2;
        }
        Ok(Chan(desc))
    }
}

/// Channel string parse failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChanError {
    /// Empty string.
    Empty,
    /// Not a whole number of letter+digit pairs.
    OddLength,
    /// More channels than fit a u32 descriptor.
    TooManyChannels,
    /// Unknown channel letter (or non-digit), from `channames = "rgbkamx"`.
    BadChar(char),
    /// Channel width outside 1..=8.
    BadDepth(u32),
    /// Total depth exceeds 32 bits.
    TooDeep(u32),
}

impl fmt::Display for ChanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChanError::Empty => write!(f, "empty channel string"),
            ChanError::OddLength => write!(f, "channel string is not letter+digit pairs"),
            ChanError::TooManyChannels => write!(f, "more than {} channels", MAX_CHANNELS),
            ChanError::BadChar(c) => {
                write!(f, "bad channel character {:?} (want rgbkamx + digit)", c)
            }
            ChanError::BadDepth(d) => write!(f, "channel depth {} outside 1..=8", d),
            ChanError::TooDeep(d) => write!(f, "total channel depth {} exceeds 32 bits", d),
        }
    }
}

impl std::error::Error for ChanError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_canonical_descriptors() {
        assert_eq!(Chan::from_str("x8r8g8b8"), Ok(Chan::XRGB32));
        assert_eq!(Chan::from_str("r8g8b8"), Ok(Chan::RGB24));
        assert_eq!(Chan::from_str("k1"), Ok(Chan::GREY1));
        assert_eq!(Chan::from_str("m8"), Ok(Chan::CMAP8));
        assert_eq!(Chan::from_str("a8r8g8b8"), Ok(Chan::ARGB32));
        assert_eq!(Chan::from_str("r5g6b5"), Ok(Chan::RGB16));
    }

    #[test]
    fn display_matches_chan_strings() {
        assert_eq!(Chan::XRGB32.to_string(), "x8r8g8b8");
        assert_eq!(Chan::RGB24.to_string(), "r8g8b8");
        assert_eq!(Chan::GREY1.to_string(), "k1");
        assert_eq!(Chan::CMAP8.to_string(), "m8");
        assert_eq!(Chan::ARGB32.to_string(), "a8r8g8b8");
        assert_eq!(Chan::RGB16.to_string(), "r5g6b5");
        assert_eq!(Chan::RGBA32.to_string(), "r8g8b8a8");
    }

    #[test]
    fn depth_sums_channel_widths() {
        assert_eq!(Chan::XRGB32.depth(), 32);
        assert_eq!(Chan::RGB24.depth(), 24);
        assert_eq!(Chan::RGB16.depth(), 16);
        assert_eq!(Chan::GREY8.depth(), 8);
        assert_eq!(Chan::GREY1.depth(), 1);
    }

    #[test]
    fn rejects_malformed_descriptors() {
        assert_eq!(Chan::from_str(""), Err(ChanError::Empty));
        // "r8g8" is a VALID even-length descriptor (SPEC.md §7; plan9port
        // strtochan accepts it → 0x0818), so it must parse, not fail with
        // OddLength. Use the genuinely odd-length "r8g" for this case.
        assert_eq!(Chan::from_str("r8g"), Err(ChanError::OddLength));
        assert_eq!(Chan::from_str("z8"), Err(ChanError::BadChar('z')));
        assert_eq!(Chan::from_str("r0"), Err(ChanError::BadDepth(0)));
        assert_eq!(Chan::from_str("r9"), Err(ChanError::BadDepth(9)));
        assert_eq!(
            Chan::from_str("r8g8b8a8x8"),
            Err(ChanError::TooManyChannels)
        );
    }
}
