//! Tests for UTF-8 BOM handling, in particular for BOM signatures that are
//! split across several calls, and for interactions with one-byte output
//! buffers and one-slot field-end buffers.

use csv_core::{ReadFieldResult, ReadRecordResult, Reader, ReaderBuilder};

type Csv = Vec<Vec<Vec<u8>>>;

fn reader(nfa: bool) -> Reader {
    let mut b = ReaderBuilder::new();
    b.nfa(nfa);
    b.build()
}

/// Drive `read_field` over the given input chunks with an output buffer that
/// starts at `out_cap` bytes and grows only when reported full.
fn parse_by_field(nfa: bool, chunks: &[Vec<u8>], out_cap: usize) -> Csv {
    let mut rdr = reader(nfa);
    let mut out = vec![0u8; out_cap];
    let mut pos = 0usize;
    let (mut recs, mut row): (Csv, Vec<Vec<u8>>) = (Vec::new(), Vec::new());

    let feed = |rdr: &mut Reader,
                data_in: &[u8],
                out: &mut Vec<u8>,
                pos: &mut usize,
                row: &mut Vec<Vec<u8>>,
                recs: &mut Csv|
     -> bool {
        let mut data = data_in;
        loop {
            let (res, nin, nout) = rdr.read_field(data, &mut out[*pos..]);
            data = &data[nin..];
            *pos += nout;
            match res {
                ReadFieldResult::InputEmpty => {
                    assert!(data.is_empty(), "unread input remains");
                    return false;
                }
                ReadFieldResult::OutputFull => {
                    out.resize(core::cmp::max(1, out.len() * 2), 0);
                    // The current chunk is exhausted: wait for the next one.
                    // Retrying now with an empty slice would falsely signal
                    // end of stream.
                    if data.is_empty() {
                        return false;
                    }
                }
                ReadFieldResult::Field { record_end } => {
                    row.push(out[..*pos].to_vec());
                    *pos = 0;
                    if record_end {
                        recs.push(std::mem::take(row));
                    }
                    // Chunk exhausted at a field boundary: wait for more.
                    if data.is_empty() {
                        return false;
                    }
                }
                ReadFieldResult::End => return true,
            }
        }
    };

    for chunk in chunks {
        if feed(&mut rdr, chunk, &mut out, &mut pos, &mut row, &mut recs) {
            return recs;
        }
    }
    // Stream truly exhausted: an empty slice now signals end.
    loop {
        let (res, _, nout) = rdr.read_field(&[], &mut out[pos..]);
        pos += nout;
        match res {
            ReadFieldResult::InputEmpty => {
                unreachable!("empty input cannot be InputEmpty here")
            }
            ReadFieldResult::OutputFull => {
                out.resize(core::cmp::max(1, out.len() * 2), 0)
            }
            ReadFieldResult::Field { record_end } => {
                row.push(out[..pos].to_vec());
                pos = 0;
                if record_end {
                    recs.push(std::mem::take(&mut row));
                }
            }
            ReadFieldResult::End => return recs,
        }
    }
}

/// Drive `read_record` over the given input chunks with output/end buffers
/// that start at `out_cap`/`ends_cap` slots and grow only when reported full.
fn parse_by_record(
    nfa: bool,
    chunks: &[Vec<u8>],
    out_cap: usize,
    ends_cap: usize,
) -> Csv {
    let mut rdr = reader(nfa);
    let mut out = vec![0u8; out_cap];
    let mut ends = vec![0usize; ends_cap];
    let (mut outpos, mut endpos) = (0usize, 0usize);
    let mut recs = Csv::new();

    let feed = |rdr: &mut Reader,
                data_in: &[u8],
                out: &mut Vec<u8>,
                ends: &mut Vec<usize>,
                outpos: &mut usize,
                endpos: &mut usize,
                recs: &mut Csv|
     -> bool {
        let mut data = data_in;
        loop {
            let (res, nin, nout, nend) = rdr.read_record(
                data,
                &mut out[*outpos..],
                &mut ends[*endpos..],
            );
            data = &data[nin..];
            *outpos += nout;
            *endpos += nend;
            match res {
                ReadRecordResult::InputEmpty => {
                    assert!(data.is_empty(), "unread input remains");
                    return false;
                }
                ReadRecordResult::OutputFull => {
                    out.resize(core::cmp::max(1, out.len() * 2), 0);
                    if data.is_empty() {
                        return false;
                    }
                }
                ReadRecordResult::OutputEndsFull => {
                    ends.resize(core::cmp::max(1, ends.len() * 2), 0);
                    if data.is_empty() {
                        return false;
                    }
                }
                ReadRecordResult::Record => {
                    let mut start = 0;
                    let mut row = Vec::new();
                    for &e in &ends[..*endpos] {
                        row.push(out[start..e].to_vec());
                        start = e;
                    }
                    recs.push(row);
                    *outpos = 0;
                    *endpos = 0;
                    if data.is_empty() {
                        return false;
                    }
                }
                ReadRecordResult::End => return true,
            }
        }
    };

    for chunk in chunks {
        if feed(
            &mut rdr,
            chunk,
            &mut out,
            &mut ends,
            &mut outpos,
            &mut endpos,
            &mut recs,
        ) {
            return recs;
        }
    }
    // Stream truly exhausted: an empty slice now signals end.
    loop {
        let (res, _, nout, nend) =
            rdr.read_record(&[], &mut out[outpos..], &mut ends[endpos..]);
        outpos += nout;
        endpos += nend;
        match res {
            ReadRecordResult::InputEmpty => {
                unreachable!("empty input cannot be InputEmpty here")
            }
            ReadRecordResult::OutputFull => {
                out.resize(core::cmp::max(1, out.len() * 2), 0)
            }
            ReadRecordResult::OutputEndsFull => {
                ends.resize(core::cmp::max(1, ends.len() * 2), 0)
            }
            ReadRecordResult::Record => {
                let mut start = 0;
                let mut row = Vec::new();
                for &e in &ends[..endpos] {
                    row.push(out[start..e].to_vec());
                    start = e;
                }
                recs.push(row);
                outpos = 0;
                endpos = 0;
            }
            ReadRecordResult::End => return recs,
        }
    }
}

fn chunks_of(data: &[u8], size: usize) -> Vec<Vec<u8>> {
    data.chunks(size).map(|c| c.to_vec()).collect()
}

fn s(csv: &Csv) -> Vec<Vec<String>> {
    csv.iter()
        .map(|r| {
            r.iter().map(|f| String::from_utf8(f.clone()).unwrap()).collect()
        })
        .collect()
}

const CORPUS: &[&[u8]] = &[
    b"",
    b"\xef\xbb\xbf",
    b"\xef",
    b"\xef\xbb",
    b"\xef\xbb\xbfname,value\r\n\xe7\x94\xb2,7\r\n",
    b"\xef\xbb\xbfa",
    b"\xef\xbb,x\nok,y\n",
    b"\xef,x\nok,y\n",
    b"a,\xef\xbb\xbfb\n\xef\xbb\xbfc,d\n",
    b"\xef\xbb\xbf\xef\xbb\xbfa\n",
    b"x\ny\nz\n",
    b"\xef\xbb\xbf\n",
    b"\xef\xbb\xbf\"a,b\",\"c\"\n",
    b"\xef\xbb\xbf\"x\"",
    b"\xef\xbb\xbf,a\n",
    b"\xef\xbb\xbf\n\nx\n",
    b"\xef\xbb\xbfa\r\nb\r\n",
];

#[test]
fn chunked_equals_whole() {
    for &data in CORPUS {
        for nfa in [false, true] {
            let whole_f = parse_by_field(nfa, &chunks_of(data, 64), 64);
            let whole_r = parse_by_record(nfa, &chunks_of(data, 64), 64, 8);
            assert_eq!(whole_f, whole_r, "whole mismatch: {:?}", data);

            // Every input chunking from a single byte upward must produce the
            // same fields/records/order as reading the data at once, even with
            // one-byte output buffers and one field-end slot.
            for size in 1..=core::cmp::max(1, data.len()) {
                let chunks = chunks_of(data, size);
                let by_field = parse_by_field(nfa, &chunks, 1);
                assert_eq!(
                    by_field, whole_f,
                    "field nfa={} size={} data={:?}",
                    nfa, size, data
                );
                let by_record = parse_by_record(nfa, &chunks, 1, 1);
                assert_eq!(
                    by_record, whole_r,
                    "record nfa={} size={} data={:?}",
                    nfa, size, data
                );
            }

            // Larger output capacities exercise different flush orderings.
            for cap in [1usize, 2, 3, 5, 32] {
                assert_eq!(
                    parse_by_field(nfa, &chunks_of(data, 1), cap),
                    whole_f,
                    "field cap nfa={} cap={} data={:?}",
                    nfa,
                    cap,
                    data
                );
                assert_eq!(
                    parse_by_record(nfa, &chunks_of(data, 1), cap, cap),
                    whole_r,
                    "record cap nfa={} cap={} data={:?}",
                    nfa,
                    cap,
                    data
                );
            }
        }
    }
}

#[test]
fn bom_stripped_exact_fields() {
    let data = b"\xef\xbb\xbfname,value\r\n\xe7\x94\xb2,7\r\n";
    let expected: Vec<Vec<String>> = vec![
        vec!["name".to_string(), "value".to_string()],
        vec!["甲".to_string(), "7".to_string()],
    ];
    for nfa in [false, true] {
        assert_eq!(
            s(&parse_by_field(nfa, &chunks_of(data, 64), 64)),
            expected
        );
        assert_eq!(
            s(&parse_by_record(nfa, &chunks_of(data, 64), 64, 8)),
            expected
        );
    }
}

// The BOM split as 1+1+1, 1+2 and 2+1 bytes must be stripped identically.
#[test]
fn bom_split_field() {
    let expected: Vec<Vec<String>> = vec![vec!["a".into(), "b".into()]];
    let splits: &[&[&[u8]]] = &[
        &[b"\xef", b"\xbb", b"\xbfa,b\n"],
        &[b"\xef", b"\xbb\xbf", b"a,b\n"],
        &[b"\xef\xbb", b"\xbf", b"a,b\n"],
        &[b"\xef\xbb\xbf", b"a,b\n"],
        &[b"\xef", b"\xbb", b"\xbf", b"a,b\n"],
    ];
    for nfa in [false, true] {
        for split in splits {
            let chunks: Vec<Vec<u8>> =
                split.iter().map(|c| c.to_vec()).collect();
            assert_eq!(
                s(&parse_by_field(nfa, &chunks, 1)),
                expected,
                "field nfa={} split={:?}",
                nfa,
                split
            );
            assert_eq!(
                s(&parse_by_record(nfa, &chunks, 1, 1)),
                expected,
                "record nfa={} split={:?}",
                nfa,
                split
            );
        }
    }
}

// A first chunk that is exactly the complete BOM must not report end or emit
// an empty field; a BOM-only file has no records at all.
#[test]
fn bom_only_then_data_and_bom_only_file() {
    for nfa in [false, true] {
        // BOM then data.
        let got =
            parse_by_field(nfa, &[b"\xef\xbb\xbf".to_vec(), b"a".to_vec()], 1);
        assert_eq!(s(&got), vec![vec!["a".to_string()]]);
        let got = parse_by_record(
            nfa,
            &[b"\xef\xbb\xbf".to_vec(), b"a".to_vec()],
            1,
            1,
        );
        assert_eq!(s(&got), vec![vec!["a".to_string()]]);

        // BOM only: no records.
        assert!(parse_by_field(nfa, &[b"\xef\xbb\xbf".to_vec()], 1).is_empty());
        assert!(
            parse_by_record(nfa, &[b"\xef\xbb\xbf".to_vec()], 1, 1).is_empty()
        );
        // Empty file: no records.
        assert!(parse_by_field(nfa, &[], 1).is_empty());
        assert!(parse_by_record(nfa, &[], 1, 1).is_empty());
    }
}

// Explicit accounting for read_field: input consumption only counts bytes
// actually received, and a retained BOM prefix is consumed immediately even
// though nothing reaches the output.
#[test]
fn bom_split_field_accounting() {
    use ReadFieldResult::*;
    for nfa in [false, true] {
        let mut rdr = reader(nfa);
        let mut out = [0u8; 1];

        let (res, nin, nout) = rdr.read_field(b"\xef", &mut out);
        assert_eq!(res, InputEmpty);
        assert_eq!(nin, 1);
        assert_eq!(nout, 0);

        let (res, nin, nout) = rdr.read_field(b"\xbb", &mut out);
        assert_eq!(res, InputEmpty);
        assert_eq!(nin, 1);
        assert_eq!(nout, 0);

        // Third byte completes the BOM; following data parses one byte.
        let (res, nin, nout) = rdr.read_field(b"\xbfab", &mut out);
        assert_eq!(res, OutputFull);
        assert_eq!(nin, 2); // BOM byte + 'a'
        assert_eq!(nout, 1);
        assert_eq!(&out[..1], b"a");

        let (res, nin, nout) = rdr.read_field(b"b", &mut out);
        assert_eq!(res, InputEmpty);
        assert_eq!(nin, 1);
        assert_eq!(nout, 1);
        assert_eq!(&out[..1], b"b");

        let (res, _, _) = rdr.read_field(&[], &mut out);
        assert_eq!(res, Field { record_end: true });
        let (res, _, _) = rdr.read_field(&[], &mut out);
        assert_eq!(res, End);
    }
}

// Empty output buffer exactly when the prefix is released: the released
// bytes stay staged until space is available and no data is lost or repeated.
#[test]
fn empty_buffer_at_prefix_release() {
    use ReadFieldResult::*;
    for nfa in [false, true] {
        let mut rdr = reader(nfa);
        let mut out = [0u8; 4];

        // Two retained prefix bytes.
        let (res, nin, _) = rdr.read_field(b"\xef", &mut out);
        assert_eq!((res, nin), (InputEmpty, 1));
        let (res, nin, _) = rdr.read_field(b"\xbb", &mut out);
        assert_eq!((res, nin), (InputEmpty, 1));

        // Mismatch byte while the output buffer has no space: the released
        // prefix cannot be written yet. Only the comma is accepted.
        let (res, nin, nout) = rdr.read_field(b",x\n", &mut []);
        assert_eq!(res, OutputFull);
        assert_eq!((nin, nout), (1, 0));

        // Retry with the remainder (`x\n`) and space: the staged prefix
        // flushes first and ends the first field.
        let (res, nin, nout) = rdr.read_field(b"x\n", &mut out);
        assert_eq!(res, Field { record_end: false });
        assert_eq!((nin, nout), (0, 2));
        assert_eq!(&out[..2], b"\xef\xbb");

        // The remainder is still owed and yields the second field.
        let (res, nin, nout) = rdr.read_field(b"x\n", &mut out);
        assert_eq!(res, Field { record_end: true });
        assert_eq!((nin, nout), (2, 1));
        assert_eq!(&out[..1], b"x");

        let (res, _, _) = rdr.read_field(&[], &mut out);
        assert_eq!(res, End);
    }
}

// Explicit accounting for read_record: a complete BOM as the first chunk
// consumes three bytes and produces no output and no field ends; field ends
// are measured as if the BOM never existed.
#[test]
fn bom_only_chunk_record_accounting() {
    use ReadRecordResult::*;
    for nfa in [false, true] {
        let mut rdr = reader(nfa);
        let mut out = [0u8; 1];
        let mut ends = [0usize; 1];

        let (res, nin, nout, nend) =
            rdr.read_record(b"\xef\xbb\xbf", &mut out, &mut ends);
        assert_eq!(res, InputEmpty);
        assert_eq!(nin, 3);
        assert_eq!(nout, 0);
        assert_eq!(nend, 0);

        // First data byte after the confirmed BOM.
        let (res, nin, nout, _) = rdr.read_record(b"ab", &mut out, &mut ends);
        assert_eq!(res, OutputFull);
        assert_eq!(nin, 1);
        assert_eq!(nout, 1);

        // Finish the record. The 'a' from the previous call lives in the
        // caller's earlier buffer; field ends use logical record positions
        // (`output_pos`), hence 2 and 3.
        let mut buf = [0u8; 8];
        let mut ep = [0usize; 4];
        let (res, nin, nout, nend) =
            rdr.read_record(b"b,c\n", &mut buf, &mut ep);
        assert_eq!(res, Record);
        assert_eq!(nin, 4);
        assert_eq!(nout, 2);
        assert_eq!(nend, 2);
        assert_eq!(&buf[..2], b"bc");
        // Field ends exclude the stripped BOM and count the earlier 'a'.
        assert_eq!(ep[0], 2);
        assert_eq!(ep[1], 3);
    }
}

// Empty output/end buffers at arbitrary points make no progress and lose no
// data. The BOM-completing byte is consumed by detection even when the output
// buffer is empty (it produced no output); subsequent retries continue with
// the bytes that remain.
#[test]
fn empty_buffers_during_bom() {
    use ReadRecordResult::*;
    for nfa in [false, true] {
        let mut rdr = reader(nfa);
        let data: &[u8] = b"\xef\xbb\xbfab,cd\n";
        let one = &mut [0u8; 1];
        let one_end = &mut [0usize; 1];

        // Retain the first two BOM bytes one at a time.
        let (res, nin, nout, nend) = rdr.read_record(&data[..1], one, one_end);
        assert_eq!(res, InputEmpty);
        assert_eq!((nin, nout, nend), (1, 0, 0));
        let (res, nin, nout, nend) =
            rdr.read_record(&data[1..2], one, one_end);
        assert_eq!(res, InputEmpty);
        assert_eq!((nin, nout, nend), (1, 0, 0));

        // Empty output buffer while the third BOM byte completes the
        // signature: the signature byte is consumed by detection, but the
        // parser writes nothing.
        let (res, nin, nout, nend) =
            rdr.read_record(&data[2..], &mut [], one_end);
        assert_eq!(res, OutputFull);
        assert_eq!((nin, nout, nend), (1, 0, 0));

        // Empty ends buffer: detection is finished, so nothing is consumed.
        let (res, nin, nout, nend) = rdr.read_record(&data[3..], one, &mut []);
        assert_eq!(res, OutputEndsFull);
        assert_eq!((nin, nout, nend), (0, 0, 0));

        // The rest of the record parses intact with tiny buffers.
        let got = parse_with(rdr, &data[3..]);
        assert_eq!(s(&got), vec![vec!["ab".to_string(), "cd".to_string()]]);
    }
}

// A partial signature followed by a mismatching byte keeps the received
// bytes as ordinary data. This is the `EF BB ,x` / `ok,y` scenario, and the
// first record must contain exactly the raw bytes EF BB and x.
#[test]
fn partial_signature_is_data() {
    let data: &[u8] = b"\xef\xbb,x\nok,y\n";
    let expected_raw: Csv = vec![
        vec![b"\xef\xbb".to_vec(), b"x".to_vec()],
        vec![b"ok".to_vec(), b"y".to_vec()],
    ];
    for nfa in [false, true] {
        // Whole, split and single-byte deliveries.
        let deliveries: Vec<Vec<Vec<u8>>> = vec![
            chunks_of(data, 64),
            chunks_of(data, 1),
            vec![b"\xef".to_vec(), b"\xbb,x\nok,y\n".to_vec()],
            vec![b"\xef\xbb".to_vec(), b",x\nok,y\n".to_vec()],
            vec![b"\xef\xbb".to_vec(), b",x\n".to_vec(), b"ok,y\n".to_vec()],
        ];
        for chunks in deliveries {
            assert_eq!(
                parse_by_field(nfa, &chunks, 1),
                expected_raw,
                "field nfa={}",
                nfa
            );
            assert_eq!(
                parse_by_record(nfa, &chunks, 1, 1),
                expected_raw,
                "record nfa={}",
                nfa
            );
        }
    }
}

// A dangling partial signature at end of input is preserved verbatim: no
// swallowing, no replacement characters.
#[test]
fn dangling_prefix_at_end_is_data() {
    let cases: &[&[u8]] = &[b"\xef", b"\xef\xbb"];
    for data in cases {
        for nfa in [false, true] {
            let expected: Csv = vec![vec![data.to_vec()]];
            assert_eq!(
                parse_by_field(nfa, &chunks_of(data, 1), 1),
                expected,
                "{:?}",
                data
            );
            assert_eq!(
                parse_by_record(nfa, &chunks_of(data, 1), 1, 1),
                expected,
                "{:?}",
                data
            );
        }
    }
}

// The same three bytes at a later field or record start are always kept.
#[test]
fn bom_bytes_later_are_kept() {
    let data: &[u8] = b"a,\xef\xbb\xbfb\n\xef\xbb\xbfc,d\n";
    let expected: Vec<Vec<String>> = vec![
        vec!["a".into(), "\u{feff}b".into()],
        vec!["\u{feff}c".into(), "d".into()],
    ];
    for nfa in [false, true] {
        for size in [1usize, 2, 3, 64] {
            assert_eq!(
                s(&parse_by_field(nfa, &chunks_of(data, size), 1)),
                expected
            );
            assert_eq!(
                s(&parse_by_record(nfa, &chunks_of(data, size), 1, 1)),
                expected
            );
        }
    }
}

// reset() re-enables detection and discards an unfinished prefix.
#[test]
fn reset_clears_prefix_and_redetects() {
    use ReadFieldResult::*;
    for nfa in [false, true] {
        let mut rdr = reader(nfa);
        let out = &mut [0u8; 8];

        // Begin a BOM but never finish it.
        let (res, nin, _) = rdr.read_field(b"\xef", out);
        assert_eq!(res, InputEmpty);
        assert_eq!(nin, 1);

        rdr.reset();

        // Fresh data starting with a BOM is stripped; the earlier EF must not
        // bleed into it.
        let (res, nin, _) = rdr.read_field(b"\xef\xbb\xbfab", out);
        assert_eq!(res, InputEmpty);
        assert_eq!(nin, 5);
        assert_eq!(&out[..2], b"ab");

        // A reset followed by an unrelated byte starts clean as well.
        rdr.reset();
        let (res, _, _) = rdr.read_field(b"z", out);
        assert_eq!(res, InputEmpty);
        assert_eq!(&out[..1], b"z");
    }
}

// Disabling detection explicitly preserves a leading signature (used after a
// seek to a non-zero record offset).
#[test]
fn disabled_detection_keeps_signature() {
    for nfa in [false, true] {
        let mut rdr = reader(nfa);
        rdr.set_bom_checking(false);
        let got = parse_with(rdr, b"\xef\xbb\xbfa\n");
        assert_eq!(s(&got), vec![vec!["\u{feff}a".to_string()]]);
    }
}

fn parse_with(mut rdr: Reader, data: &[u8]) -> Csv {
    use ReadRecordResult::*;
    let mut out = vec![0u8; 1];
    let mut ends = vec![0usize; 1];
    let (mut op, mut ep) = (0usize, 0usize);
    let mut recs = Csv::new();
    let mut rest = data;
    loop {
        let (res, nin, nout, nend) =
            rdr.read_record(rest, &mut out[op..], &mut ends[ep..]);
        rest = &rest[nin..];
        op += nout;
        ep += nend;
        match res {
            InputEmpty => {
                if rest.is_empty() {
                    rest = &[];
                }
            }
            OutputFull => out.resize(out.len() * 2, 0),
            OutputEndsFull => ends.resize(ends.len() * 2, 0),
            Record => {
                let mut start = 0;
                let mut row = Vec::new();
                for &e in &ends[..ep] {
                    row.push(out[start..e].to_vec());
                    start = e;
                }
                recs.push(row);
                op = 0;
                ep = 0;
            }
            End => break,
        }
    }
    recs
}
