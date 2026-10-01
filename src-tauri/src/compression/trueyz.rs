//! Byte-exact Yaz0 encoder: a Rust port of TrueYZ
//! (https://github.com/aboood40091/TrueYZ, MIT License, © AboodXD), the
//! reconstruction of Nintendo's in-house zlib-derived lazy-matching Yaz0
//! encoder that Switch Toolbox compresses with (`trueyz.dll`).
//!
//! The match-finder constants and the shape of the search are not free
//! parameters: changing any of them still yields valid Yaz0, but no longer
//! the reference bytes.

const HEADER_SIZE: usize = 16;
const MIN_MATCH: u32 = 3;
const LONG_FORM_MIN: u32 = MIN_MATCH + 15;
const MAX_MATCH: u32 = LONG_FORM_MIN + 255;
const WSIZE: u32 = 4096;
const WMASK: u32 = WSIZE - 1;
/// zlib uses `MAX_MATCH + MIN_MATCH + 1`; the reference encoder omits the +1.
const MIN_LOOKAHEAD: u32 = MAX_MATCH + MIN_MATCH;
/// The full 12-bit distance, not zlib's `WSIZE - MIN_LOOKAHEAD`.
const MAX_DIST: u32 = 4096;
const HASH_LOG: u32 = 15;
const HASH_SIZE: usize = 1 << HASH_LOG;
const HASH_MASK: u32 = (HASH_SIZE as u32) - 1;
const HASH_SHIFT: u32 = (HASH_LOG + MIN_MATCH - 1) / MIN_MATCH;
const GOOD_MATCH: u32 = 32;
const MAX_LAZY: u32 = 32;
const NICE_MATCH: u32 = MAX_MATCH;
const MAX_CHAIN: u32 = 4096;
const NIL: u32 = u32::MAX;

struct Writer {
    out: Vec<u8>,
    flag_position: usize,
    /// Bit for the next token; 0 means no flag group is open.
    mask: u8,
}

impl Writer {
    fn ensure_group(&mut self) {
        if self.mask == 0 {
            self.flag_position = self.out.len();
            self.out.push(0);
            self.mask = 0x80;
        }
    }

    fn literal(&mut self, value: u8) {
        self.ensure_group();
        self.out[self.flag_position] |= self.mask;
        self.out.push(value);
        self.mask >>= 1;
    }

    fn reference(&mut self, length: u32, distance: u32) {
        self.ensure_group();
        let distance = distance - 1;
        if length < LONG_FORM_MIN {
            let code = ((length - (MIN_MATCH - 1)) << 12) | distance;
            self.out.push((code >> 8) as u8);
            self.out.push(code as u8);
        } else {
            self.out.push(((distance >> 8) & 0x0f) as u8);
            self.out.push(distance as u8);
            self.out.push((length - LONG_FORM_MIN) as u8);
        }
        self.mask >>= 1;
    }
}

struct Matcher<'a> {
    /// Most recent position (+1, 0 = empty) whose 3-byte hash is the index.
    head: Vec<u32>,
    /// Previous position (+1) sharing the hash of `position & WMASK`.
    prev: Vec<u32>,
    data: &'a [u8],
    hash: u32,
}

impl Matcher<'_> {
    fn insert(&mut self, position: u32) -> u32 {
        if position as usize + MIN_MATCH as usize > self.data.len() {
            return NIL;
        }
        self.hash = ((self.hash << HASH_SHIFT)
            ^ u32::from(self.data[(position + MIN_MATCH - 1) as usize]))
            & HASH_MASK;
        let tag = self.head[self.hash as usize];
        self.prev[(position & WMASK) as usize] = tag;
        self.head[self.hash as usize] = position + 1;
        if tag >= 1 {
            tag - 1
        } else {
            NIL
        }
    }

    fn longest_match(
        &self,
        position: u32,
        head: u32,
        previous_length: u32,
        chain_limit: u32,
        match_position: &mut u32,
    ) -> u32 {
        let length = self.data.len() as u32;
        let lookahead = length - position;
        if previous_length >= lookahead {
            return lookahead;
        }
        let mut chain_length = MAX_CHAIN;
        let mut best_length = previous_length;
        let nice_length = NICE_MATCH.min(lookahead);
        let scan = position as usize;
        let limit = if position + MAX_MATCH < length {
            scan + MAX_MATCH as usize
        } else {
            length as usize
        };
        let mut current = head;
        if previous_length >= GOOD_MATCH {
            chain_length >>= 2;
        }
        loop {
            let candidate = current as usize;
            if self.data[candidate + best_length as usize] == self.data[scan + best_length as usize]
            {
                let mut matched = 0usize;
                while scan + matched < limit
                    && self.data[candidate + matched] == self.data[scan + matched]
                {
                    matched += 1;
                }
                if matched as u32 > best_length {
                    best_length = matched as u32;
                    *match_position = current;
                    if best_length >= nice_length {
                        break;
                    }
                }
            }
            // Chains run strictly backwards; a link that does not is stale.
            let tag = self.prev[(current & WMASK) as usize];
            let next = if tag >= 1 { tag - 1 } else { NIL };
            if next == NIL || next >= current {
                break;
            }
            current = next;
            chain_length -= 1;
            if !(current > chain_limit && chain_length != 0) {
                break;
            }
        }
        best_length.min(lookahead)
    }
}

/// zlib's window offset of an absolute position: the 2*WSIZE buffer slides
/// down by WSIZE whenever strstart reaches `2*WSIZE - MIN_LOOKAHEAD`.
fn window_position(position: u32) -> u32 {
    let first_slide = 2 * WSIZE - MIN_LOOKAHEAD;
    if position < first_slide {
        position
    } else {
        (WSIZE - MIN_LOOKAHEAD) + ((position - first_slide) % WSIZE)
    }
}

/// Compresses `data` to a complete Yaz0 stream (16-byte header included)
/// with `alignment` stored in the header's alignment hint.
pub fn compress(data: &[u8], alignment: u32) -> Vec<u8> {
    let length = data.len() as u32;
    let mut out = Vec::with_capacity(HEADER_SIZE + data.len() + data.len() / 8 + 1);
    out.extend_from_slice(b"Yaz0");
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&alignment.to_be_bytes());
    out.extend_from_slice(&[0; 4]);
    let mut writer = Writer {
        out,
        flag_position: 0,
        mask: 0,
    };
    let mut matcher = Matcher {
        head: vec![0; HASH_SIZE],
        prev: vec![0; WSIZE as usize],
        data,
        hash: 0,
    };
    // Prime the rolling hash with the first MIN_MATCH - 1 bytes.
    for &byte in data.iter().take(2) {
        matcher.hash = ((matcher.hash << HASH_SHIFT) ^ u32::from(byte)) & HASH_MASK;
    }

    let mut position = 0u32;
    let mut match_length = MIN_MATCH - 1;
    let mut match_position = 0u32;
    // Lazy matching: a token found at `position` is held back until the
    // next position has been examined.
    let mut match_pending = false;
    while position < length {
        let head = matcher.insert(position);
        let previous_length = match_length;
        let previous_position = match_position;
        match_length = MIN_MATCH - 1;

        let offset = window_position(position);
        let window_base = position - offset;
        let chain_limit = if offset > MAX_DIST {
            position - MAX_DIST
        } else {
            window_base
        };
        if head != NIL && previous_length < MAX_LAZY && head >= chain_limit && head > window_base {
            match_length = matcher.longest_match(
                position,
                head,
                previous_length,
                chain_limit,
                &mut match_position,
            );
        }

        if previous_length >= MIN_MATCH && match_length <= previous_length {
            // The pending match wins; it started one byte back.
            writer.reference(previous_length, (position - 1) - previous_position);
            for _ in 0..previous_length - 2 {
                position += 1;
                matcher.insert(position);
            }
            match_pending = false;
            match_length = MIN_MATCH - 1;
            position += 1;
        } else if match_pending {
            writer.literal(data[(position - 1) as usize]);
            position += 1;
        } else {
            match_pending = true;
            position += 1;
        }
    }
    if match_pending {
        writer.literal(data[(position - 1) as usize]);
    }
    writer.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_a_bare_header() {
        assert_eq!(compress(&[], 0).len(), HEADER_SIZE);
    }

    #[test]
    fn round_trips_through_the_decoder() {
        let mut data = Vec::new();
        for i in 0..40_000u32 {
            data.push(((i * i) >> 7) as u8 ^ (i % 13) as u8);
        }
        data.extend(std::iter::repeat(0x5a).take(9000));
        let packed = compress(&data, 0x2000);
        assert_eq!(&packed[8..12], &0x2000u32.to_be_bytes());
        assert_eq!(roead::yaz0::decompress(&packed).unwrap(), data);
    }
}
