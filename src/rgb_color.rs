//! The single owner of RGB hex byte decoding for the whole crate.
//!
//! Extracted unchanged (body and semantics) from the decoder that used to
//! live in `candidate_state.rs`, so that `candidate_state`,
//! `theme_roles` and `gpu_gallery` all share exactly one implementation
//! instead of copied validators. The function is `pub` so that
//! `theme_roles` can keep its public compatibility re-export; the helper
//! `hex_digit` remains private to this module.
//!
//! Rule: trim outer whitespace, strip at most one optional leading `#`,
//! then require exactly six ASCII hex digits (mixed case accepted).
//! Repeated `#`, RGB shorthand, eight-digit/alpha forms and non-ASCII
//! bytes are all rejected.

pub fn parse_hex_rgb_bytes(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    let bytes = value.as_bytes();
    if bytes.len() != 6 {
        return None;
    }

    let mut channels = [0u8; 3];
    for (channel, pair) in channels.iter_mut().zip(bytes.chunks_exact(2)) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        *channel = (high << 4) | low;
    }
    Some(channels)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hex_rgb_bytes;

    #[test]
    fn rgb_color_owner_accepts_the_existing_app_forms() {
        assert_eq!(parse_hex_rgb_bytes("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex_rgb_bytes("  A1b2C3  "), Some([0xa1, 0xb2, 0xc3]));
        assert_eq!(parse_hex_rgb_bytes("\t#AaBbCc\n"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(parse_hex_rgb_bytes("000000"), Some([0, 0, 0]));
        assert_eq!(parse_hex_rgb_bytes("#FFFFFF"), Some([255, 255, 255]));
    }

    #[test]
    fn rgb_color_owner_rejects_repeated_hash_alpha_and_non_ascii() {
        for bad in [
            "",
            "#",
            "#12",
            "#12345",
            "#1234567",
            "zzzzzz",
            "#12xz45",
            "##ff8000",
            "#ff800080",
            "ff800080",
        ] {
            assert_eq!(parse_hex_rgb_bytes(bad), None, "{bad:?}");
        }
        assert_eq!(parse_hex_rgb_bytes("あabc"), None);
    }
}
