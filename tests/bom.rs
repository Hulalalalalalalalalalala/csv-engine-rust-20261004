//! High-level reader tests for UTF-8 BOM handling.

use std::io;

use csv::{ByteRecord, ErrorKind, ReaderBuilder, StringRecord};
use serde::Deserialize;

fn collect_string(
    data: &[u8],
    headers: bool,
    cap: usize,
) -> Vec<StringRecord> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(headers)
        .buffer_capacity(cap)
        .from_reader(data);
    let mut out = Vec::new();
    for rec in rdr.records() {
        out.push(rec.unwrap());
    }
    out
}

fn collect_bytes(data: &[u8], headers: bool, cap: usize) -> Vec<ByteRecord> {
    let mut rdr = ReaderBuilder::new()
        .has_headers(headers)
        .buffer_capacity(cap)
        .from_reader(data);
    let mut out = Vec::new();
    for rec in rdr.byte_records() {
        out.push(rec.unwrap());
    }
    out
}

#[test]
fn bom_headers_and_records() {
    let data: &[u8] = b"\xef\xbb\xbfname,value\r\n\xe7\x94\xb2,7\r\n";
    for cap in [1usize, 2, 3, 8192] {
        let mut rdr =
            ReaderBuilder::new().buffer_capacity(cap).from_reader(data);
        let headers = rdr.headers().unwrap();
        assert_eq!(headers, vec!["name", "value"], "cap={}", cap);

        let rec = rdr.records().next().unwrap().unwrap();
        assert_eq!(rec, vec!["甲", "7"], "cap={}", cap);
        assert!(rdr.records().next().is_none());
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct Row {
    name: String,
    value: i64,
}

#[test]
fn bom_deserialize_serde() {
    let data: &[u8] = b"\xef\xbb\xbfname,value\r\n\xe7\x94\xb2,7\r\n";
    for cap in [1usize, 2, 8192] {
        let mut rdr =
            ReaderBuilder::new().buffer_capacity(cap).from_reader(data);
        let rows: Vec<Row> =
            rdr.deserialize().collect::<csv::Result<Vec<_>>>().unwrap();
        assert_eq!(
            rows,
            vec![Row { name: "甲".to_string(), value: 7 }],
            "cap={}",
            cap
        );
    }
}

#[test]
fn bom_no_headers_two_records() {
    let data: &[u8] = b"\xef\xbb\xbfname,value\r\n\xe7\x94\xb2,7\r\n";
    for cap in [1usize, 2, 3, 8192] {
        let recs = collect_string(data, false, cap);
        assert_eq!(recs.len(), 2, "cap={}", cap);
        assert_eq!(recs[0], vec!["name", "value"]);
        assert_eq!(recs[1], vec!["甲", "7"]);
    }
}

#[test]
fn bom_only_and_empty_have_no_records() {
    for data in [&b"\xef\xbb\xbf"[..], &b""[..]] {
        for cap in [1usize, 8192] {
            assert!(collect_string(data, false, cap).is_empty());
            assert!(collect_bytes(data, false, cap).is_empty());
            let mut rdr =
                ReaderBuilder::new().buffer_capacity(cap).from_reader(data);
            let h = rdr.headers().unwrap();
            assert_eq!(h.len(), 0);
        }
    }
}

// The `EF BB ,x...` case: raw byte records keep the partial signature, and
// the second record is still readable after the first one fails UTF-8
// validation. The error location must not depend on internal chunking.
#[test]
fn partial_signature_byte_records_and_utf8_error() {
    let data: &[u8] = b"\xef\xbb,x\nok,y\n";

    for cap in [1usize, 2, 3, 8192] {
        let recs = collect_bytes(data, false, cap);
        assert_eq!(recs.len(), 2, "cap={}", cap);
        assert_eq!(recs[0].get(0), Some(&b"\xef\xbb"[..]));
        assert_eq!(recs[0].get(1), Some(&b"x"[..]));
        assert_eq!(recs[1].get(0), Some(&b"ok"[..]));
        assert_eq!(recs[1].get(1), Some(&b"y"[..]));
    }

    for cap in [1usize, 2, 3, 8192] {
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .buffer_capacity(cap)
            .from_reader(data);
        let mut rec = StringRecord::new();

        let err = rdr.read_record(&mut rec).unwrap_err();
        match *err.kind() {
            ErrorKind::Utf8 { ref pos, ref err } => {
                assert_eq!(pos.as_ref().unwrap().byte(), 0, "cap={}", cap);
                assert_eq!(err.field(), 0, "cap={}", cap);
                assert_eq!(err.valid_up_to(), 0, "cap={}", cap);
            }
            ref other => panic!("wrong error: {:?}", other),
        }

        assert!(rdr.read_record(&mut rec).unwrap(), "cap={}", cap);
        assert_eq!(rec, vec!["ok", "y"], "cap={}", cap);
        assert!(!rdr.read_record(&mut rec).unwrap());
    }
}

// The default DFA parser and the hidden NFA parser must agree, including
// when input arrives one byte at a time through a tiny reader buffer.
#[test]
fn nfa_and_dfa_agree() {
    let cases: &[&[u8]] = &[
        b"\xef\xbb\xbfname,value\r\n\xe7\x94\xb2,7\r\n",
        b"\xef\xbb,x\nok,y\n",
        b"\xef",
        b"\xef\xbb",
        b"\xef\xbb\xbf",
        b"a,\xef\xbb\xbfb\n\xef\xbb\xbfc,d\n",
    ];
    for data in cases {
        for cap in [1usize, 3, 8192] {
            let dfa_bytes = collect_bytes(data, false, cap);
            let mut rdr = ReaderBuilder::new();
            rdr.has_headers(false).buffer_capacity(cap).nfa(true);
            let nfa: Vec<ByteRecord> = rdr
                .from_reader(*data)
                .byte_records()
                .collect::<csv::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(nfa.len(), dfa_bytes.len(), "{:?} cap={}", data, cap);
            for (a, b) in nfa.iter().zip(dfa_bytes.iter()) {
                assert_eq!(
                    a.iter().collect::<Vec<_>>(),
                    b.iter().collect::<Vec<_>>(),
                    "{:?} cap={}",
                    data,
                    cap
                );
            }
        }
    }
}

// The same three bytes at a later record start are preserved.
#[test]
fn later_bom_bytes_are_data() {
    let data: &[u8] = b"\xef\xbb\xbfh\n\xef\xbb\xbfmiddle\nplain\n";
    for cap in [1usize, 2, 3, 8192] {
        let mut rdr =
            ReaderBuilder::new().buffer_capacity(cap).from_reader(data);
        assert_eq!(rdr.headers().unwrap(), vec!["h"]);

        let rec = rdr.records().next().unwrap().unwrap();
        assert_eq!(rec, vec!["\u{feff}middle"], "cap={}", cap);
        let rec = rdr.records().next().unwrap().unwrap();
        assert_eq!(rec, vec!["plain"], "cap={}", cap);
        assert!(rdr.records().next().is_none());
    }
}

// Byte offsets count the BOM bytes: the first record starts at 0, later
// positions include the three-byte marker.
#[test]
fn positions_count_bom_bytes() {
    let data: &[u8] = b"\xef\xbb\xbfa,b\nc,d\n";
    for cap in [1usize, 8192] {
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .buffer_capacity(cap)
            .from_reader(io::Cursor::new(data));

        let p0 = rdr.position().clone();
        assert_eq!((p0.byte(), p0.record()), (0, 0));

        let mut rec = ByteRecord::new();
        assert!(rdr.read_byte_record(&mut rec).unwrap());
        assert_eq!(rec, vec!["a", "b"]);

        // BOM (3) + "a,b\n" (4) = 7.
        let p1 = rdr.position().clone();
        assert_eq!((p1.byte(), p1.record()), (7, 1), "cap={}", cap);

        assert!(rdr.read_byte_record(&mut rec).unwrap());
        assert_eq!(rec, vec!["c", "d"]);
    }
}

// Seeking to a non-zero record must not strip matching bytes, while seeking
// back to byte 0 strips the BOM again.
#[test]
fn seek_preserves_bom_bytes_away_from_zero() {
    let data: &[u8] = b"\xef\xbb\xbfa\n\xef\xbb\xbfb\nc\n";
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(io::Cursor::new(data));

    let mut rec = StringRecord::new();
    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["a"]);
    let pos_b = rdr.position().clone();

    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["\u{feff}b"]);

    // Seek to the second record: its leading BOM must be retained.
    rdr.seek(pos_b.clone()).unwrap();
    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["\u{feff}b"], "BOM stripped after seek");

    // Seek back to zero using the recorded (original-file) offset: the BOM at
    // the file start is stripped once more.
    let mut zero = csv::Position::new();
    zero.set_byte(0).set_line(1).set_record(0);
    rdr.seek(zero).unwrap();
    assert!(rdr.read_record(&mut rec).unwrap());
    assert_eq!(rec, vec!["a"]);

    let _ = pos_b;
}
