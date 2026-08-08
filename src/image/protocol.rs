//! Terminal graphics escape sequences.
//!
//! Both protocols take the original file bytes and decode them terminal side,
//! so nothing here interprets image data. Every sequence produced here leaves
//! the cursor where it was, and the caller decides the position.

/// Graphics protocol a terminal understands. Detection is by environment
/// variable only; the terminal is never queried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
    Iterm2,
}

/// The largest base64 payload a single kitty escape may carry.
const KITTY_CHUNK: usize = 4096;

const ST: &str = "\x1b\\";
/// DECSC / DECRC. The iTerm2 sequence advances the cursor past the image, so it
/// is bracketed to keep the "does not move the cursor" contract.
const SAVE_CURSOR: &str = "\x1b7";
const RESTORE_CURSOR: &str = "\x1b8";

/// Draws `bytes` at the cursor, occupying `cols` x `rows` cells.
pub fn place(protocol: Protocol, bytes: &[u8], cols: usize, rows: usize) -> String {
    let payload = base64(bytes);
    match protocol {
        Protocol::Kitty => kitty(&payload, cols, rows),
        Protocol::Iterm2 => format!(
            "{SAVE_CURSOR}\x1b]1337;File=inline=1;size={};width={cols};height={rows};preserveAspectRatio=1:{payload}\x07{RESTORE_CURSOR}",
            bytes.len()
        ),
    }
}

/// Removes every image the terminal is showing, or `None` when the protocol has
/// no such command and the caller must repaint the cells instead.
pub fn clear(protocol: Protocol) -> Option<&'static str> {
    match protocol {
        Protocol::Kitty => Some("\x1b_Ga=d,d=A\x1b\\"),
        Protocol::Iterm2 => None,
    }
}

/// `a=T` transmits and displays in one step, `f=100` says the payload is a
/// complete image file, and `C=1` keeps the cursor still. Payloads are sent in
/// chunks because a single escape is length limited.
fn kitty(payload: &str, cols: usize, rows: usize) -> String {
    let mut out = String::with_capacity(payload.len() + 64);
    let mut chunks = payload
        .as_bytes()
        .chunks(KITTY_CHUNK)
        .map(|chunk| std::str::from_utf8(chunk).expect("base64 is ascii"))
        .peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        if first {
            out.push_str(&format!(
                "\x1b_Ga=T,f=100,c={cols},r={rows},C=1,m={more};{chunk}{ST}"
            ));
            first = false;
        } else {
            out.push_str(&format!("\x1b_Gm={more};{chunk}{ST}"));
        }
    }
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding. Written out rather than taken from a crate:
/// this is the only place `mdvu` needs it.
fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let b = [
            group[0],
            *group.get(1).unwrap_or(&0),
            *group.get(2).unwrap_or(&0),
        ];
        let bits = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= group.len() {
                out.push(ALPHABET[((bits >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_test_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn a_short_kitty_payload_is_one_escape() {
        let out = place(Protocol::Kitty, b"foobar", 10, 5);
        assert_eq!(out, "\x1b_Ga=T,f=100,c=10,r=5,C=1,m=0;Zm9vYmFy\x1b\\");
    }

    #[test]
    fn a_long_kitty_payload_is_chunked_and_terminated() {
        let bytes = vec![0u8; KITTY_CHUNK * 2];
        let out = place(Protocol::Kitty, &bytes, 4, 2);
        assert_eq!(out.matches("\x1b_G").count(), 3);
        assert!(out.starts_with("\x1b_Ga=T,f=100,c=4,r=2,C=1,m=1;"));
        assert!(out.contains("\x1b_Gm=1;"));
        // Only the final chunk clears the "more data follows" flag.
        assert_eq!(out.matches("m=0;").count(), 1);
        assert!(out.ends_with(ST));
    }

    #[test]
    fn the_iterm2_sequence_carries_the_byte_count_and_cell_size() {
        let out = place(Protocol::Iterm2, b"foobar", 10, 5);
        assert!(out.contains("size=6;"));
        assert!(out.contains("width=10;height=5;"));
        assert!(out.contains(":Zm9vYmFy\x07"));
    }

    /// The caller positions the cursor itself, so neither sequence may leave it
    /// somewhere else.
    #[test]
    fn neither_protocol_moves_the_cursor() {
        let kitty = place(Protocol::Kitty, b"foobar", 1, 1);
        assert!(kitty.contains("C=1"));
        let iterm2 = place(Protocol::Iterm2, b"foobar", 1, 1);
        assert!(iterm2.starts_with(SAVE_CURSOR));
        assert!(iterm2.ends_with(RESTORE_CURSOR));
    }

    #[test]
    fn only_kitty_can_delete_what_it_drew() {
        assert_eq!(clear(Protocol::Kitty), Some("\x1b_Ga=d,d=A\x1b\\"));
        assert_eq!(clear(Protocol::Iterm2), None);
    }
}
