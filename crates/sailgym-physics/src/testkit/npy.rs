//! A bounded `.npy` v1.0 codec for `f64` matrices (section 02 task 2.4).
//!
//! The conformance bundle is **data**, and the format it is data in has to be
//! readable by every stack that will ever be held against it. `.npy` v1.0 is
//! a 64-byte-aligned ASCII header followed by raw little-endian elements:
//! about forty lines to write, about thirty to read, and `numpy.load` opens
//! it directly with no dependency at all.
//!
//! ## Why not `.npz`
//!
//! `cross-stack.md` proposed one `.npz` archive. `.npz` is a zip container,
//! and this workspace's entire dependency graph is `serde` and `serde_json`
//! (plus, since task 2.1, an optional `sha2` behind the `testkit` feature).
//! A zip crate to hold files that a directory already holds is a dependency
//! bought for nothing. Recorded as a deviation from the note in
//! `docs/v2/progress/02-handoff.md`.
//!
//! ## Why the codec lives in `testkit` and not in the generator
//!
//! Two consumers read the same bytes: `crates/sailgym-bench`'s generator
//! writes them, and `crates/sailgym-physics/tests/conformance.rs` reads them
//! back to check the committed bundle against current source. A physics test
//! may not depend on the bench crate — that is a dependency cycle, and F8.1
//! keeps the physics crate's graph minimal on purpose — so the shared piece
//! lives here, behind the same `testkit` feature as the rest of this module,
//! and nothing of it reaches `wasm-pack build`.
//!
//! ## Bounded before allocating
//!
//! Every rejection below happens **before** a buffer is reserved. A header
//! claiming a `(2^40, 2^40)` shape is a corrupt file, not an allocation
//! request; [`MAX_ELEMENTS`] is the ceiling and the dtype, the shape, the
//! ordering and the payload length are all checked against each other first.

use std::fmt;

/// The six magic bytes every `.npy` starts with.
const MAGIC: &[u8] = b"\x93NUMPY";

/// Bytes before the header text: magic, two version bytes, and the `u16`
/// header length.
const PREAMBLE: usize = MAGIC.len() + 2 + 2;

/// `.npy` v1.0 pads the header so that the data begins on a 64-byte boundary.
const ALIGN: usize = 64;

/// The dtype this codec writes and the only one it reads: little-endian
/// IEEE-754 binary64, which is what F1 says the physics core is made of.
const DTYPE: &str = "<f8";

/// The largest matrix this codec will allocate for, in elements — 16 Mi
/// `f64`, i.e. 128 MiB.
///
/// The whole bundle is budgeted at 2 MB (task 2.4), so this is three orders
/// of magnitude of headroom; it exists only so that a corrupt or hostile
/// header cannot turn into an allocation of whatever number it happens to
/// contain.
pub const MAX_ELEMENTS: usize = 16 << 20;

/// Why a `.npy` was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NpyError {
    /// Not a `.npy` at all, or truncated before the header could be read.
    Magic(String),
    /// A version this codec does not write.
    Version(u8, u8),
    /// The header dictionary could not be parsed, or claims something this
    /// codec does not implement.
    Header(String),
    /// The declared shape and the payload length disagree.
    Length {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    /// The declared shape is larger than [`MAX_ELEMENTS`].
    TooLarge { elements: usize },
}

impl fmt::Display for NpyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Magic(why) => write!(f, "not a .npy file: {why}"),
            Self::Version(a, b) => write!(f, ".npy version {a}.{b}; this codec reads 1.0 only"),
            Self::Header(why) => write!(f, "unreadable .npy header: {why}"),
            Self::Length {
                expected_bytes,
                actual_bytes,
            } => write!(
                f,
                ".npy payload is {actual_bytes} bytes, but the declared shape needs \
                 {expected_bytes}"
            ),
            Self::TooLarge { elements } => write!(
                f,
                ".npy declares {elements} elements, above the {MAX_ELEMENTS} this codec \
                 will allocate for"
            ),
        }
    }
}

impl std::error::Error for NpyError {}

/// A 2-D `f64` matrix, row-major, as the bundle stores it.
#[derive(Clone, Debug, PartialEq)]
pub struct Npy2d {
    pub rows: usize,
    pub cols: usize,
    /// `rows * cols` elements, row-major.
    pub data: Vec<f64>,
}

impl Npy2d {
    /// Row `i`, as a slice.
    pub fn row(&self, i: usize) -> &[f64] {
        &self.data[i * self.cols..(i + 1) * self.cols]
    }
}

/// Encode a row-major `f64` matrix as `.npy` v1.0.
///
/// # Panics
///
/// If `data.len() != rows * cols`. That is a caller bug, not a file defect:
/// the writer is the generator, and a generator that miscounted its own
/// columns must stop rather than write a plausible-looking fixture.
pub fn encode_f64_2d(rows: usize, cols: usize, data: &[f64]) -> Vec<u8> {
    assert_eq!(
        data.len(),
        rows * cols,
        "encode_f64_2d: {} values for a ({rows}, {cols}) matrix",
        data.len()
    );
    // numpy writes a one-element tuple as `(n,)`; a two-element one has no
    // trailing comma. Matching it exactly keeps the file byte-identical to
    // what `numpy.save` would have produced, which is the cheapest possible
    // guarantee that `numpy.load` will read it.
    let dict =
        format!("{{'descr': '{DTYPE}', 'fortran_order': False, 'shape': ({rows}, {cols}), }}");
    // Pad with spaces so the data starts on a 64-byte boundary, and end the
    // header with the newline the format requires.
    let unpadded = PREAMBLE + dict.len() + 1;
    let padding = (ALIGN - unpadded % ALIGN) % ALIGN;
    let header_len = dict.len() + padding + 1;

    let mut out = Vec::with_capacity(PREAMBLE + header_len + data.len() * 8);
    out.extend_from_slice(MAGIC);
    out.push(1);
    out.push(0);
    out.extend_from_slice(&(header_len as u16).to_le_bytes());
    out.extend_from_slice(dict.as_bytes());
    out.extend(std::iter::repeat_n(b' ', padding));
    out.push(b'\n');
    for v in data {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Read a value out of the header dictionary: `'key': <value>`, up to the
/// next comma at nesting depth zero.
fn dict_value<'a>(dict: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("'{key}':");
    let start = dict.find(&needle)? + needle.len();
    let rest = &dict[start..];
    let mut depth = 0usize;
    for (i, ch) in rest.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth > 0 => depth -= 1,
            ')' | ']' | '}' => return Some(rest[..i].trim()),
            ',' if depth == 0 => return Some(rest[..i].trim()),
            _ => {}
        }
    }
    Some(rest.trim())
}

/// Decode a `.npy` v1.0 holding a 2-D `<f8` array.
///
/// Every check below runs before anything is allocated.
pub fn decode_f64_2d(bytes: &[u8]) -> Result<Npy2d, NpyError> {
    if bytes.len() < PREAMBLE {
        return Err(NpyError::Magic(format!(
            "{} bytes, fewer than the {PREAMBLE}-byte preamble",
            bytes.len()
        )));
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(NpyError::Magic("wrong magic".to_string()));
    }
    let (major, minor) = (bytes[MAGIC.len()], bytes[MAGIC.len() + 1]);
    if (major, minor) != (1, 0) {
        return Err(NpyError::Version(major, minor));
    }
    let header_len = u16::from_le_bytes([bytes[MAGIC.len() + 2], bytes[MAGIC.len() + 3]]) as usize;
    let data_at = PREAMBLE + header_len;
    if bytes.len() < data_at {
        return Err(NpyError::Magic(format!(
            "header claims {header_len} bytes, file has {}",
            bytes.len() - PREAMBLE
        )));
    }
    if !data_at.is_multiple_of(ALIGN) {
        return Err(NpyError::Header(format!(
            "data starts at byte {data_at}, which is not a multiple of {ALIGN}"
        )));
    }
    let dict = std::str::from_utf8(&bytes[PREAMBLE..data_at])
        .map_err(|_| NpyError::Header("header is not UTF-8".to_string()))?;

    let descr = dict_value(dict, "descr")
        .ok_or_else(|| NpyError::Header("no `descr`".to_string()))?
        .trim_matches('\'');
    if descr != DTYPE {
        return Err(NpyError::Header(format!(
            "dtype `{descr}`; this codec reads `{DTYPE}` only"
        )));
    }
    let order = dict_value(dict, "fortran_order")
        .ok_or_else(|| NpyError::Header("no `fortran_order`".to_string()))?;
    if order != "False" {
        return Err(NpyError::Header(format!(
            "fortran_order is `{order}`; this codec reads row-major only"
        )));
    }
    let shape =
        dict_value(dict, "shape").ok_or_else(|| NpyError::Header("no `shape`".to_string()))?;
    let inner = shape
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| NpyError::Header(format!("shape `{shape}` is not a tuple")))?;
    let dims: Vec<usize> = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| NpyError::Header(format!("shape entry `{s}` is not a count")))
        })
        .collect::<Result<_, _>>()?;
    if dims.len() != 2 {
        return Err(NpyError::Header(format!(
            "shape ({inner}) is {}-dimensional; the bundle is 2-D throughout",
            dims.len()
        )));
    }
    let (rows, cols) = (dims[0], dims[1]);

    // Bounds, then length, then — and only then — a buffer.
    let elements = rows.checked_mul(cols).ok_or(NpyError::TooLarge {
        elements: usize::MAX,
    })?;
    if elements > MAX_ELEMENTS {
        return Err(NpyError::TooLarge { elements });
    }
    let expected_bytes = elements * 8;
    let actual_bytes = bytes.len() - data_at;
    if actual_bytes != expected_bytes {
        return Err(NpyError::Length {
            expected_bytes,
            actual_bytes,
        });
    }

    let mut data = Vec::with_capacity(elements);
    let (chunks, _) = bytes[data_at..].as_chunks::<8>();
    for chunk in chunks {
        data.push(f64::from_le_bytes(*chunk));
    }
    Ok(Npy2d { rows, cols, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Splice a replacement into the header, **as bytes**, space-padded to
    /// the original length so the file's declared header length and its
    /// 64-byte alignment both stay correct.
    ///
    /// Not through a `String`: the magic begins with `0x93`, which is not
    /// valid UTF-8, so a lossy round trip would rewrite the file before the
    /// edit under test even landed.
    fn rewrite(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
        assert!(to.len() <= from.len(), "the edit must fit the original");
        let padded = format!("{to}{}", " ".repeat(from.len() - to.len()));
        let mut out = bytes.to_vec();
        let at = out
            .windows(from.len())
            .position(|w| w == from.as_bytes())
            .unwrap_or_else(|| panic!("the fixture must contain {from}"));
        out[at..at + padded.len()].copy_from_slice(padded.as_bytes());
        out
    }

    /// A syntactically valid `.npy` whose header declares `shape` and whose
    /// payload holds `values` elements.
    ///
    /// Built from scratch rather than by editing a good file, because the
    /// shapes worth rejecting are longer than the one they replace.
    fn with_shape(shape: &str, values: usize) -> Vec<u8> {
        let dict = format!("{{'descr': '{DTYPE}', 'fortran_order': False, 'shape': {shape}, }}");
        let unpadded = PREAMBLE + dict.len() + 1;
        let padding = (ALIGN - unpadded % ALIGN) % ALIGN;
        let header_len = dict.len() + padding + 1;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1);
        out.push(0);
        out.extend_from_slice(&(header_len as u16).to_le_bytes());
        out.extend_from_slice(dict.as_bytes());
        out.extend(std::iter::repeat_n(b' ', padding));
        out.push(b'\n');
        for i in 0..values {
            out.extend_from_slice(&(i as f64).to_le_bytes());
        }
        out
    }

    fn matrix(rows: usize, cols: usize) -> Vec<f64> {
        (0..rows * cols).map(|i| (i as f64) * 0.25 - 3.0).collect()
    }

    #[test]
    fn round_trips_bit_for_bit() {
        for (rows, cols) in [(0, 5), (1, 1), (3, 7), (129, 2), (17, 23)] {
            let data = matrix(rows, cols);
            let bytes = encode_f64_2d(rows, cols, &data);
            assert_eq!(bytes.len() % ALIGN, (rows * cols * 8) % ALIGN);
            let back = decode_f64_2d(&bytes).expect("round trip");
            assert_eq!((back.rows, back.cols), (rows, cols));
            for (a, b) in data.iter().zip(back.data.iter()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }
    }

    /// Every value a bundle can hold has to survive, including the ones a
    /// text format would mangle.
    #[test]
    fn the_awkward_values_survive() {
        let data = vec![
            0.0,
            -0.0,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            f64::MAX,
            f64::MIN,
            f64::EPSILON,
            std::f64::consts::PI,
            1.0e-300,
            -1.0e300,
        ];
        let bytes = encode_f64_2d(1, data.len(), &data);
        let back = decode_f64_2d(&bytes).expect("round trip");
        for (a, b) in data.iter().zip(back.data.iter()) {
            assert_eq!(a.to_bits(), b.to_bits(), "{a}");
        }
        // Signed zero in particular: `-0.0 == 0.0` in IEEE, so only the bit
        // comparison above can see it.
        assert_eq!(back.data[1].to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn the_header_is_what_numpy_writes() {
        let bytes = encode_f64_2d(3, 4, &matrix(3, 4));
        assert_eq!(&bytes[..6], b"\x93NUMPY");
        assert_eq!((bytes[6], bytes[7]), (1, 0));
        let header_len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        assert_eq!((PREAMBLE + header_len) % ALIGN, 0, "data must be aligned");
        let header = std::str::from_utf8(&bytes[PREAMBLE..PREAMBLE + header_len]).unwrap();
        assert!(header.ends_with('\n'));
        assert!(header.contains("'descr': '<f8'"), "{header}");
        assert!(header.contains("'fortran_order': False"), "{header}");
        assert!(header.contains("'shape': (3, 4)"), "{header}");
    }

    #[test]
    fn malformed_files_are_refused_by_name() {
        let good = encode_f64_2d(4, 3, &matrix(4, 3));

        // Not a .npy.
        assert!(matches!(
            decode_f64_2d(b"not an npy at all, really"),
            Err(NpyError::Magic(_))
        ));
        assert!(matches!(decode_f64_2d(b""), Err(NpyError::Magic(_))));
        assert!(matches!(decode_f64_2d(&good[..4]), Err(NpyError::Magic(_))));

        // A version this codec does not write.
        let mut v2 = good.clone();
        v2[6] = 2;
        assert_eq!(decode_f64_2d(&v2), Err(NpyError::Version(2, 0)));

        // Truncated payload, and one element too many.
        let short = &good[..good.len() - 8];
        assert!(matches!(decode_f64_2d(short), Err(NpyError::Length { .. })));
        let mut long = good.clone();
        long.extend_from_slice(&0.0f64.to_le_bytes());
        assert!(matches!(decode_f64_2d(&long), Err(NpyError::Length { .. })));

        // Wrong dtype, wrong ordering, wrong rank — each rewritten into a
        // header of exactly the same length, so the *only* thing wrong with
        // the file is the thing being tested.
        for (from, to) in [
            ("'descr': '<f8'", "'descr': '<f4'"),
            ("'descr': '<f8'", "'descr': '>f8'"),
            ("'fortran_order': False", "'fortran_order': True"),
        ] {
            let edited = rewrite(&good, from, to);
            assert_eq!(edited.len(), good.len(), "the edit changed the length");
            assert!(
                matches!(decode_f64_2d(&edited), Err(NpyError::Header(_))),
                "accepted a header with {to}"
            );
        }

        // Rank: the bundle is 2-D throughout, so a 1-D or 3-D array is a
        // different artifact wearing the same name.
        for (shape, values) in [("(12,)", 12), ("(4, 3, 1)", 12), ("()", 1)] {
            assert!(
                matches!(
                    decode_f64_2d(&with_shape(shape, values)),
                    Err(NpyError::Header(_))
                ),
                "accepted shape {shape}"
            );
        }
        // And the same builder round-trips a good shape, so the rejections
        // above are about the rank and not about the builder.
        assert!(decode_f64_2d(&with_shape("(4, 3)", 12)).is_ok());
    }

    /// A corrupt shape is a corrupt file, not an allocation request. If this
    /// check were removed the test would not fail — it would exhaust memory,
    /// so the bound is asserted on the *error*, before any buffer exists.
    #[test]
    fn an_absurd_shape_is_refused_before_allocating() {
        match decode_f64_2d(&with_shape("(99999999, 99999)", 0)) {
            Err(NpyError::TooLarge { elements }) => assert!(elements > MAX_ELEMENTS),
            other => panic!("expected TooLarge, got {other:?}"),
        }
        // And an overflowing product, which must not wrap into a small one.
        match decode_f64_2d(&with_shape("(18446744073709551615, 4)", 0)) {
            Err(NpyError::TooLarge { .. }) => {}
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn errors_say_what_is_wrong() {
        for e in [
            NpyError::Magic("wrong magic".into()),
            NpyError::Version(2, 0),
            NpyError::Header("dtype".into()),
            NpyError::Length {
                expected_bytes: 96,
                actual_bytes: 88,
            },
            NpyError::TooLarge { elements: 1 << 40 },
        ] {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn rows_are_addressable() {
        let m = decode_f64_2d(&encode_f64_2d(3, 4, &matrix(3, 4))).unwrap();
        assert_eq!(m.row(1), &matrix(3, 4)[4..8]);
    }
}
