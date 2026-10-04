//! RandomAccessSimple interaction with a leading UTF-8 BOM.

use std::io;

use csv_index::RandomAccessSimple;

fn data() -> Vec<u8> {
    let mut d = vec![];
    d.extend_from_slice(b"\xef\xbb\xbf"); // BOM at byte 0
    d.extend_from_slice(b"h1,h2\n");
    d.extend_from_slice(b"a,b\n");
    d.extend_from_slice("\u{feff}c,d\n".as_bytes()); // later record starts with BOM
    d.extend_from_slice(b"e,f\n");
    d
}

#[test]
fn random_access_with_bom() {
    for cap in [1usize, 2, 8192] {
        let data = data();

        // Sequential records, with headers enabled (the first row is hidden).
        let mut seq_rdr = csv::ReaderBuilder::new()
            .buffer_capacity(cap)
            .from_reader(io::Cursor::new(data.clone()));
        let mut sequential = Vec::new();
        for rec in seq_rdr.records() {
            sequential.push(rec.unwrap());
        }

        // Build and immediately reopen the index.
        let mut idx_rdr = csv::ReaderBuilder::new()
            .buffer_capacity(cap)
            .from_reader(io::Cursor::new(data.clone()));
        let mut buf = io::Cursor::new(Vec::new());
        RandomAccessSimple::create(&mut idx_rdr, &mut buf).unwrap();
        buf.set_position(0);
        let mut idx = RandomAccessSimple::open(buf).unwrap();

        // One index entry per record, header included.
        assert_eq!(idx.len(), 4, "cap={}", cap);

        // Header position is 0; subsequent offsets include the BOM.
        assert_eq!(idx.get(0).unwrap().byte(), 0);
        assert_eq!(idx.get(1).unwrap().byte(), 9);
        assert_eq!(idx.get(2).unwrap().byte(), 13);
        assert_eq!(idx.get(3).unwrap().byte(), 20);

        // A fresh reader used only for seeking. Index 0 strips the file BOM;
        // the later BOM-leading record keeps its bytes.
        let mut seek_rdr = csv::Reader::from_reader(io::Cursor::new(data));

        let header_pos = idx.get(0).unwrap();
        seek_rdr.seek(header_pos).unwrap();
        let h = seek_rdr.records().next().unwrap().unwrap();
        assert_eq!(h, vec!["h1", "h2"], "cap={}", cap);

        let p2 = idx.get(2).unwrap();
        seek_rdr.seek(p2).unwrap();
        let r2 = seek_rdr.records().next().unwrap().unwrap();
        assert_eq!(r2, vec!["\u{feff}c", "d"], "cap={}", cap);

        // Every indexed record read out of order matches the sequential read.
        let expected = [
            vec!["h1".to_string(), "h2".to_string()],
            vec!["a".to_string(), "b".to_string()],
            vec!["\u{feff}c".to_string(), "d".to_string()],
            vec!["e".to_string(), "f".to_string()],
        ];
        for i in 0..idx.len() {
            let pos = idx.get(i).unwrap();
            seek_rdr.seek(pos).unwrap();
            let got = seek_rdr.records().next().unwrap().unwrap();
            assert_eq!(got, expected[i as usize], "i={} cap={}", i, cap);
        }

        // Sequential non-header records agree with positions 1..=3.
        assert_eq!(sequential.len(), 3);
        for (i, rec) in sequential.iter().enumerate() {
            let pos = idx.get(i as u64 + 1).unwrap();
            seek_rdr.seek(pos).unwrap();
            let got = seek_rdr.records().next().unwrap().unwrap();
            assert_eq!(&got, rec, "i={} cap={}", i, cap);
        }
    }
}

// An index created with headers disabled indexes the BOM-less first row at 0
// just the same.
#[test]
fn random_access_with_bom_no_headers() {
    let data = data();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(io::Cursor::new(data.clone()));
    let mut buf = io::Cursor::new(Vec::new());
    RandomAccessSimple::create(&mut rdr, &mut buf).unwrap();
    buf.set_position(0);
    let mut idx = RandomAccessSimple::open(buf).unwrap();
    assert_eq!(idx.len(), 4);

    let mut seek_rdr = csv::Reader::from_reader(io::Cursor::new(data));
    for i in 0..idx.len() {
        let pos = idx.get(i).unwrap();
        seek_rdr.seek(pos).unwrap();
        let mut rec = csv::ByteRecord::new();
        assert!(seek_rdr.read_byte_record(&mut rec).unwrap());
        if i == 0 {
            assert_eq!(rec, vec!["h1", "h2"]);
        } else if i == 2 {
            assert_eq!(rec.get(0), Some("\u{feff}c".as_bytes()));
        }
    }
}
