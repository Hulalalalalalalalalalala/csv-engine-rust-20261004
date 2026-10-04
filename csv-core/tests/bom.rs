//! Tests for incremental UTF-8 BOM stripping.
//!
//! These exercise both the field and record reading APIs, under both the DFA
//! (the default) and the NFA, with the BOM split across input buffers in
//! every possible way and with one-byte output buffers.

use csv_core::{ReadFieldResult, ReadRecordResult, Reader, ReaderBuilder};

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Collect records by `read_field`, feeding `data` in `chunk`-sized input
/// buffers and with an output buffer of `outcap` bytes.
fn by_field(
    nfa: bool,
    data: &[u8],
    chunk: usize,
    outcap: usize,
) -> Vec<Vec<Vec<u8>>> {
    let mut rdr = builder(nfa);
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let parts = data.chunks(chunk.max(1)).collect::<Vec<_>>();
    let mut idx = 0;
    loop {
        let inp = parts.get(idx).copied().unwrap_or(b"");
        idx += 1;
        let mut pos = 0;
        loop {
            let mut out = vec![0u8; outcap.max(1)];
            let (res, nin, nout) = rdr.read_field(&inp[pos..], &mut out);
            pos += nin;
            field.extend_from_slice(&out[..nout]);
            match res {
                ReadFieldResult::InputEmpty => break,
                ReadFieldResult::OutputFull => {}
                ReadFieldResult::Field { record_end } => {
                    row.push(std::mem::take(&mut field));
                    if record_end {
                        records.push(std::mem::take(&mut row));
                    }
                    if pos >= inp.len() {
                        break;
                    }
                }
                ReadFieldResult::End => return records,
            }
        }
    }
}

/// Collect records by `read_record`, feeding `data` in `chunk`-sized input
/// buffers, with an output buffer of `outcap` bytes and an ends buffer of
/// `endcap` positions.
fn by_record(
    nfa: bool,
    data: &[u8],
    chunk: usize,
    outcap: usize,
    endcap: usize,
) -> Vec<Vec<Vec<u8>>> {
    let mut rdr = builder(nfa);
    let mut outbuf = Vec::new();
    let mut ends = Vec::new();
    let mut records = Vec::new();
    let parts = data.chunks(chunk.max(1)).collect::<Vec<_>>();
    let mut idx = 0;
    loop {
        let inp = parts.get(idx).copied().unwrap_or(b"");
        idx += 1;
        let mut pos = 0;
        loop {
            let mut out = vec![0u8; outcap.max(1)];
            let mut es = vec![0usize; endcap.max(1)];
            let (res, nin, nout, nend) =
                rdr.read_record(&inp[pos..], &mut out, &mut es);
            pos += nin;
            outbuf.extend_from_slice(&out[..nout]);
            ends.extend_from_slice(&es[..nend]);
            match res {
                ReadRecordResult::InputEmpty => break,
                ReadRecordResult::OutputFull
                | ReadRecordResult::OutputEndsFull => {}
                ReadRecordResult::Record => {
                    let mut row = Vec::new();
                    let mut start = 0;
                    for &end in &ends {
                        row.push(outbuf[start..end].to_vec());
                        start = end;
                    }
                    records.push(row);
                    outbuf.clear();
                    ends.clear();
                    if pos >= inp.len() {
                        break;
                    }
                }
                ReadRecordResult::End => return records,
            }
        }
    }
}

fn builder(nfa: bool) -> Reader {
    let mut b = ReaderBuilder::new();
    b.nfa(nfa);
    b.build()
}

// Like `by_record`, but on every other parser call the output and ends
// buffers are empty, exercising the requirement that a zero-capacity buffer
// during BOM confirmation/exclusion must neither lose data nor end the parse.
fn by_record_empty_buffers(
    nfa: bool,
    data: &[u8],
    chunk: usize,
) -> Vec<Vec<Vec<u8>>> {
    let mut rdr = builder(nfa);
    let mut outbuf = Vec::new();
    let mut ends = Vec::new();
    let mut records = Vec::new();
    let parts = data.chunks(chunk.max(1)).collect::<Vec<_>>();
    let mut idx = 0;
    let mut call = 0usize;
    loop {
        let inp = parts.get(idx).copied().unwrap_or(b"");
        idx += 1;
        let mut pos = 0;
        loop {
            call += 1;
            let (ocap, ecap) = if call.is_multiple_of(2) { (0, 0) } else { (1, 1) };
            let mut out = vec![0u8; ocap];
            let mut es = vec![0usize; ecap];
            let (res, nin, nout, nend) =
                rdr.read_record(&inp[pos..], &mut out, &mut es);
            pos += nin;
            outbuf.extend_from_slice(&out[..nout]);
            ends.extend_from_slice(&es[..nend]);
            match res {
                ReadRecordResult::InputEmpty => break,
                ReadRecordResult::OutputFull
                | ReadRecordResult::OutputEndsFull => {}
                ReadRecordResult::Record => {
                    let mut row = Vec::new();
                    let mut start = 0;
                    for &end in &ends {
                        row.push(outbuf[start..end].to_vec());
                        start = end;
                    }
                    records.push(row);
                    outbuf.clear();
                    ends.clear();
                    if pos >= inp.len() {
                        break;
                    }
                }
                ReadRecordResult::End => return records,
            }
        }
    }
}

// Like `by_field` with alternating empty / one-byte output buffers.
fn by_field_empty_buffers(
    nfa: bool,
    data: &[u8],
    chunk: usize,
) -> Vec<Vec<Vec<u8>>> {
    let mut rdr = builder(nfa);
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let parts = data.chunks(chunk.max(1)).collect::<Vec<_>>();
    let mut idx = 0;
    let mut call = 0usize;
    loop {
        let inp = parts.get(idx).copied().unwrap_or(b"");
        idx += 1;
        let mut pos = 0;
        loop {
            call += 1;
            let ocap = if call.is_multiple_of(2) { 0 } else { 1 };
            let mut out = vec![0u8; ocap];
            let (res, nin, nout) = rdr.read_field(&inp[pos..], &mut out);
            pos += nin;
            field.extend_from_slice(&out[..nout]);
            match res {
                ReadFieldResult::InputEmpty => break,
                ReadFieldResult::OutputFull => {}
                ReadFieldResult::Field { record_end } => {
                    row.push(std::mem::take(&mut field));
                    if record_end {
                        records.push(std::mem::take(&mut row));
                    }
                    if pos >= inp.len() {
                        break;
                    }
                }
                ReadFieldResult::End => return records,
            }
        }
    }
}

fn printable(recs: &[Vec<Vec<u8>>]) -> Vec<Vec<String>> {
    recs.iter()
        .map(|r| {
            r.iter().map(|f| String::from_utf8_lossy(f).into_owned()).collect()
        })
        .collect()
}

fn for_each_mode(
    data: &[u8],
    check: impl Fn(Vec<Vec<Vec<u8>>>, bool, usize, usize, usize),
) {
    let max = data.len().max(1) + 2;
    for nfa in [false, true] {
        for chunk in 1..=max {
            for outcap in [1usize, 3, 64] {
                let got = by_field(nfa, data, chunk, outcap);
                check(got, nfa, chunk, outcap, 0);
                for endcap in [1usize, 4] {
                    let got = by_record(nfa, data, chunk, outcap, endcap);
                    check(got, nfa, chunk, outcap, endcap);
                }
            }
        }
    }
}

#[test]
fn bom_then_data() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"name,value\r\n\xe7\x94\xb2,7\r\n");
    let expected = vec![
        vec!["name".to_string(), "value".to_string()],
        vec!["甲".to_string(), "7".to_string()],
    ];
    for_each_mode(&data, |got, nfa, chunk, outcap, endcap| {
        let which = if endcap == 0 { "field" } else { "record" };
        assert_eq!(
            printable(&got),
            expected,
            "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
        );
    });
}

#[test]
fn bom_split_1_1_1() {
    // Each BOM byte in its own chunk, followed by data.
    let data: Vec<u8> =
        BOM.iter().copied().chain(b"a,b\n".iter().copied()).collect();
    let expected = vec![vec!["a".to_string(), "b".to_string()]];
    for_each_mode(&data, |got, nfa, chunk, outcap, endcap| {
        let which = if endcap == 0 { "field" } else { "record" };
        // The splits matter only at chunk size 1, but running every size is
        // harmless.
        assert_eq!(
            printable(&got),
            expected,
            "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
        );
    });
}

#[test]
fn bom_only_chunk_has_no_record() {
    // The first chunk is exactly the complete BOM. It must not produce an
    // empty record or an early End; the end is signaled by an empty input.
    for nfa in [false, true] {
        // By field.
        let mut rdr = builder(nfa);
        let mut out = [0u8; 1];
        let (res, nin, nout) = rdr.read_field(&BOM, &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 3);
        assert_eq!(nout, 0);
        let (res, _, _) = rdr.read_field(b"", &mut out);
        assert_eq!(res, ReadFieldResult::End);

        // By record.
        let mut rdr = builder(nfa);
        let mut out = [0u8; 1];
        let mut ends = [0usize; 1];
        let (res, nin, nout, nend) =
            rdr.read_record(&BOM, &mut out, &mut ends);
        assert_eq!(res, ReadRecordResult::InputEmpty);
        assert_eq!(nin, 3);
        assert_eq!(nout, 0);
        assert_eq!(nend, 0);
        let (res, _, _, _) = rdr.read_record(b"", &mut out, &mut ends);
        assert_eq!(res, ReadRecordResult::End);
    }
}

#[test]
fn bom_only_file_and_empty_have_no_records() {
    for nfa in [false, true] {
        for chunk in 1..=4 {
            assert!(by_field(nfa, &BOM, chunk, 1).is_empty());
            assert!(by_record(nfa, &BOM, chunk, 1, 1).is_empty());
            assert!(by_field(nfa, b"", chunk, 1).is_empty());
            assert!(by_record(nfa, b"", chunk, 1, 1).is_empty());
        }
    }
}

#[test]
fn partial_prefix_at_eof_is_data() {
    for data in [&[0xEFu8][..], &[0xEF, 0xBB][..]] {
        let expected = vec![vec![String::from_utf8_lossy(data).into_owned()]];
        for_each_mode(data, |got, nfa, chunk, outcap, endcap| {
            let which = if endcap == 0 { "field" } else { "record" };
            assert_eq!(
                printable(&got),
                expected,
                "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
            );
        });
    }
}

#[test]
fn prefix_then_mismatch_is_preserved() {
    // EF BB , x \n ok , y \n
    let data =
        [0xEFu8, 0xBB, b',', b'x', b'\n', b'o', b'k', b',', b'y', b'\n'];
    let efbb = String::from_utf8_lossy(&[0xEF, 0xBB]).into_owned();
    let expected = vec![
        vec![efbb, "x".to_string()],
        vec!["ok".to_string(), "y".to_string()],
    ];
    for_each_mode(&data, |got, nfa, chunk, outcap, endcap| {
        let which = if endcap == 0 { "field" } else { "record" };
        assert_eq!(
            printable(&got),
            expected,
            "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
        );
    });

    // A single EF followed by a comma: EF is the first field.
    let data = [0xEFu8, b',', b'x', b'\n'];
    let expected = vec![vec![
        String::from_utf8_lossy(&[0xEF]).into_owned(),
        "x".to_string(),
    ]];
    for_each_mode(&data, |got, nfa, chunk, outcap, endcap| {
        let which = if endcap == 0 { "field" } else { "record" };
        assert_eq!(
            printable(&got),
            expected,
            "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
        );
    });
}

#[test]
fn later_bom_is_data() {
    let data = b"a\n\xef\xbb\xbfb\n";
    let expected = vec![vec!["a".to_string()], vec!["\u{feff}b".to_string()]];
    for_each_mode(data, |got, nfa, chunk, outcap, endcap| {
        let which = if endcap == 0 { "field" } else { "record" };
        assert_eq!(
            printable(&got),
            expected,
            "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
        );
    });

    // A BOM inside a later field is kept too.
    let data = b"a,\xef\xbb\xbfb\n";
    let expected = vec![vec!["a".to_string(), "\u{feff}b".to_string()]];
    for_each_mode(data, |got, nfa, chunk, outcap, endcap| {
        let which = if endcap == 0 { "field" } else { "record" };
        assert_eq!(
            printable(&got),
            expected,
            "{which} nfa={nfa} chunk={chunk} outcap={outcap} endcap={endcap}"
        );
    });
}

#[test]
fn zero_capacity_buffers_during_detection() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"name,value\r\n\xe7\x94\xb2,7\r\n");
    let expected = vec![
        vec!["name".to_string(), "value".to_string()],
        vec!["甲".to_string(), "7".to_string()],
    ];
    for nfa in [false, true] {
        for chunk in 1..=8 {
            assert_eq!(
                printable(&by_field_empty_buffers(nfa, &data, chunk)),
                expected,
                "field nfa={nfa} chunk={chunk}"
            );
            assert_eq!(
                printable(&by_record_empty_buffers(nfa, &data, chunk)),
                expected,
                "record nfa={nfa} chunk={chunk}"
            );
        }
    }
}

#[test]
fn field_end_positions_exclude_bom() {
    for nfa in [false, true] {
        let mut rdr = builder(nfa);
        let mut out = [0u8; 16];
        let mut ends = [0usize; 4];
        // BOM + "a,b\n" in a single buffer.
        let mut data = BOM.to_vec();
        data.extend_from_slice(b"a,b\n");
        let (res, nin, nout, nend) =
            rdr.read_record(&data, &mut out, &mut ends);
        assert_eq!(res, ReadRecordResult::Record);
        assert_eq!(nin, data.len());
        // The delimiter is discarded, so the unescaped output is "ab".
        assert_eq!(nout, 2);
        assert_eq!(nend, 2);
        assert_eq!(&out[..nout], b"ab");
        assert_eq!(ends[..2], [1, 2]);
    }
}

#[test]
fn reset_redetects_and_drops_partial_prefix() {
    for nfa in [false, true] {
        let mut rdr = builder(nfa);
        let mut out = [0u8; 4];
        // Begin an undecided BOM prefix from one stream.
        let (res, nin, _) = rdr.read_field(&[0xEF], &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 1);

        // Reset and feed a new stream. The stale prefix must not leak into
        // the new input. EF matches the first BOM byte, but the following
        // comma rules the BOM out, so EF is an ordinary field byte.
        rdr.reset();
        let (res, nin, nout) = rdr.read_field(&[0xEF, b','], &mut out);
        assert_eq!(res, ReadFieldResult::Field { record_end: false });
        assert_eq!(nin, 2);
        assert_eq!(nout, 1);
        assert_eq!(out[0], 0xEF);
    }
}

#[test]
fn reset_then_bom_is_stripped() {
    for nfa in [false, true] {
        let mut rdr = builder(nfa);
        let mut out = [0u8; 4];
        let _ = rdr.read_field(b"xyz\n", &mut out);
        rdr.reset();
        let mut data = BOM.to_vec();
        data.extend_from_slice(b"a\n");
        let (res, nin, nout) = rdr.read_field(&data, &mut out);
        assert_eq!(res, ReadFieldResult::Field { record_end: true });
        assert_eq!(nin, 5);
        assert_eq!(nout, 1);
        assert_eq!(out[0], b'a');
    }
}

#[test]
fn disabled_stripping_keeps_bom() {
    for nfa in [false, true] {
        let mut rdr = builder(nfa);
        rdr.set_strip_bom(false);
        let mut out = [0u8; 8];
        let mut data = BOM.to_vec();
        data.extend_from_slice(b"a\n");
        let (res, nin, nout) = rdr.read_field(&data, &mut out);
        assert_eq!(res, ReadFieldResult::Field { record_end: true });
        assert_eq!(nin, 5);
        assert_eq!(nout, 4);
        assert_eq!(&out[..4], &[0xEF, 0xBB, 0xBF, b'a']);
    }
}

#[test]
fn chunk_splits_one_and_two() {
    // Explicitly verify the 1+2 and 2+1 splits byte by byte.
    for nfa in [false, true] {
        // 1 + 2: [EF] then [BB BF], then [a \n].
        let mut rdr = builder(nfa);
        let mut out = [0u8; 8];
        let (res, nin, _) = rdr.read_field(&[0xEF], &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 1);
        let (res, nin, _) = rdr.read_field(&[0xBB, 0xBF], &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 2);
        let (res, nin, nout) = rdr.read_field(b"a\n", &mut out);
        assert_eq!(res, ReadFieldResult::Field { record_end: true });
        assert_eq!(nin, 2);
        assert_eq!(nout, 1);
        assert_eq!(out[0], b'a');

        // 2 + 1: [EF BB] then [BF], then [a \n].
        let mut rdr = builder(nfa);
        let (res, nin, _) = rdr.read_field(&[0xEF, 0xBB], &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 2);
        let (res, nin, _) = rdr.read_field(&[0xBF], &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 1);
        let (res, nin, nout) = rdr.read_field(b"a\n", &mut out);
        assert_eq!(res, ReadFieldResult::Field { record_end: true });
        assert_eq!(nin, 2);
        assert_eq!(nout, 1);
        assert_eq!(out[0], b'a');
    }
}

#[test]
fn output_full_during_replay_loses_nothing() {
    // A confirmed non-BOM prefix (EF BB then a third byte that mismatches)
    // is replayed through a one-byte output buffer, then the rest follows.
    for nfa in [false, true] {
        let mut rdr = builder(nfa);
        // Hold EF BB.
        let mut out = [0u8; 1];
        let (res, nin, _) = rdr.read_field(&[0xEF, 0xBB], &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 2);
        // The next byte mismatches. While the prefix is replayed one byte at
        // a time, that byte is not consumed.
        let (res, nin, nout) = rdr.read_field(b"z", &mut out);
        assert_eq!(res, ReadFieldResult::OutputFull);
        assert_eq!(nin, 0);
        assert_eq!(nout, 1);
        assert_eq!(out[0], 0xEF);
        let (res, nin, nout) = rdr.read_field(b"z", &mut out);
        assert_eq!(res, ReadFieldResult::OutputFull);
        assert_eq!(nin, 0);
        assert_eq!(nout, 1);
        assert_eq!(out[0], 0xBB);
        // Replay is finished; the mismatch byte is now ordinary field data.
        // As it fills the output buffer exactly as the input runs out, the
        // reader reports InputEmpty (input takes precedence), consuming it.
        let (res, nin, nout) = rdr.read_field(b"z", &mut out);
        assert_eq!(res, ReadFieldResult::InputEmpty);
        assert_eq!(nin, 1);
        assert_eq!(nout, 1);
        assert_eq!(out[0], b'z');
        // Field ends at EOF.
        let (res, _, _) = rdr.read_field(b"", &mut out);
        assert_eq!(res, ReadFieldResult::Field { record_end: true });
    }
}
