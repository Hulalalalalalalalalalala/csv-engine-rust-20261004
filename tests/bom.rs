//! High-level tests for UTF-8 BOM handling, including chunked delivery,
//! headers/serde, UTF-8 error recovery, positions and seeking.

use std::io;

use csv::{
    ByteRecord, ErrorKind, Position, Reader, ReaderBuilder, StringRecord,
};
use serde::Deserialize;

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

// Build a reader whose internal buffer holds at most `cap` bytes, so the
// underlying parser is forced to see very small input chunks.
fn reader(
    cap: usize,
    data: &[u8],
    headers: bool,
) -> Reader<io::Cursor<&[u8]>> {
    reader_nfa(cap, data, headers, false)
}

fn reader_nfa(
    cap: usize,
    data: &[u8],
    headers: bool,
    nfa: bool,
) -> Reader<io::Cursor<&[u8]>> {
    ReaderBuilder::new()
        .buffer_capacity(cap.max(1))
        .has_headers(headers)
        .nfa(nfa)
        .from_reader(io::Cursor::new(data))
}

#[derive(Deserialize, Debug, PartialEq)]
struct Row {
    name: String,
    value: u32,
}

#[test]
fn bom_with_headers_and_serde() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"name,value\r\n\xe7\x94\xb2,7\r\n");
    for cap in [1usize, 2, 3, 4, 8, 64] {
        let mut rdr = reader(cap, &data, true);
        assert_eq!(rdr.headers().unwrap(), vec!["name", "value"]);
        let row: Row = rdr.deserialize().next().unwrap().unwrap();
        assert_eq!(row, Row { name: "甲".to_string(), value: 7 });
        assert!(rdr.deserialize::<Row>().next().is_none());
    }
}

#[test]
fn bom_without_headers_yields_two_records() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"name,value\r\n\xe7\x94\xb2,7\r\n");
    for cap in [1usize, 2, 3, 5, 64] {
        let recs: Vec<StringRecord> =
            reader(cap, &data, false).records().map(|r| r.unwrap()).collect();
        assert_eq!(recs.len(), 2, "cap={cap}");
        assert_eq!(recs[0], vec!["name", "value"]);
        assert_eq!(recs[1], vec!["甲", "7"]);
    }
}

#[test]
fn bom_only_and_empty_have_no_records() {
    for cap in [1usize, 2, 3, 64] {
        assert_eq!(reader(cap, BOM, false).records().count(), 0);
        assert_eq!(reader(cap, b"", false).records().count(), 0);
    }
}

#[test]
fn truncated_prefix_then_eof_is_preserved() {
    for prefix in [&[0xEFu8][..], &[0xEF, 0xBB][..]] {
        for cap in [1usize, 2, 3, 64] {
            let mut rdr = reader(cap, prefix, false);
            let mut rec = ByteRecord::new();
            assert!(rdr.read_byte_record(&mut rec).unwrap());
            assert_eq!(rec.len(), 1);
            assert_eq!(&rec[0], prefix);
            assert!(!rdr.read_byte_record(&mut rec).unwrap());
        }
    }
}

#[test]
fn utf8_error_position_is_stable_across_chunks() {
    // EF BB , x \n ok , y \n
    let data =
        [0xEFu8, 0xBB, b',', b'x', b'\n', b'o', b'k', b',', b'y', b'\n'];
    for cap in [1usize, 2, 3, 4, 5, 10, 64] {
        // Byte records preserve the raw EF BB.
        let mut brdr = reader(cap, &data, false);
        let mut rec = ByteRecord::new();
        assert!(brdr.read_byte_record(&mut rec).unwrap());
        assert_eq!(rec.len(), 2);
        assert_eq!(&rec[0], &[0xEF, 0xBB]);
        assert_eq!(&rec[1], b"x");
        assert!(brdr.read_byte_record(&mut rec).unwrap());
        assert_eq!(&rec[0], b"ok");
        assert_eq!(&rec[1], b"y");
        assert!(!brdr.read_byte_record(&mut rec).unwrap());

        // String records report a UTF-8 error on the first record at field 0,
        // offset 0, then recover and read the second record.
        let mut srdr = reader(cap, &data, false);
        let mut srec = StringRecord::new();
        let err = srdr.read_record(&mut srec).unwrap_err();
        match err.kind() {
            ErrorKind::Utf8 { pos, err } => {
                assert_eq!(err.field(), 0);
                assert_eq!(err.valid_up_to(), 0);
                let pos = pos.as_ref().unwrap();
                assert_eq!(pos.byte(), 0);
                assert_eq!(pos.line(), 1);
                assert_eq!(pos.record(), 0);
            }
            other => panic!("expected Utf8 error, got {other:?}"),
        }
        assert!(srdr.read_record(&mut srec).unwrap());
        assert_eq!(srec, vec!["ok", "y"]);
        assert!(!srdr.read_record(&mut srec).unwrap());
    }
}

#[test]
fn nfa_matches_default_parser() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"name,value\r\n\xe7\x94\xb2,7\r\n");
    for nfa in [false, true] {
        for cap in [1usize, 2, 3, 64] {
            let mut rdr = reader_nfa(cap, &data, true, nfa);
            assert_eq!(rdr.headers().unwrap(), vec!["name", "value"]);
            let row: Row = rdr.deserialize().next().unwrap().unwrap();
            assert_eq!(row, Row { name: "甲".to_string(), value: 7 });
        }
    }

    // The EF BB mismatch scenario under the NFA as well.
    let bad = [0xEFu8, 0xBB, b',', b'x', b'\n', b'o', b'k', b',', b'y', b'\n'];
    for nfa in [false, true] {
        for cap in [1usize, 3, 64] {
            let mut rdr = reader_nfa(cap, &bad, false, nfa);
            let mut rec = ByteRecord::new();
            assert!(rdr.read_byte_record(&mut rec).unwrap());
            assert_eq!(&rec[0], &[0xEF, 0xBB]);
            assert_eq!(&rec[1], b"x");
        }
    }
}

#[test]
fn byte_positions_count_the_bom() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"a,b\nc,d\n");
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(io::Cursor::new(data));
    let mut rec = ByteRecord::new();
    assert!(rdr.read_byte_record(&mut rec).unwrap());
    assert_eq!(rec.position().unwrap().byte(), 0);
    assert!(rdr.read_byte_record(&mut rec).unwrap());
    // 3 (BOM) + 4 ("a,b\n") = 7
    assert_eq!(rec.position().unwrap().byte(), 7);
}

#[test]
fn seek_to_nonzero_offset_keeps_bom_like_record() {
    // Header "h", then a later record that begins with the BOM bytes.
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"h\n");
    data.extend_from_slice(BOM);
    data.extend_from_slice(b"v\n");

    let mut rdr = ReaderBuilder::new().from_reader(io::Cursor::new(data));
    let pos = {
        let mut rec = StringRecord::new();
        assert!(rdr.read_record(&mut rec).unwrap());
        assert_eq!(rec, vec!["\u{feff}v"]);
        rec.position().unwrap().clone()
    };
    assert_eq!(pos.byte(), 5); // BOM (3) + "h\n" (2)

    // Seeking to the non-zero offset must preserve the BOM-like sequence.
    rdr.seek(pos).unwrap();
    let mut rec = StringRecord::new();
    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["\u{feff}v"]);

    // Seeking back to byte 0 strips the leading BOM again.
    rdr.seek(Position::new()).unwrap();
    let mut rec = StringRecord::new();
    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["h"]);
    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["\u{feff}v"]);
}
