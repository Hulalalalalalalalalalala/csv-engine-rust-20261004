use std::io;

use csv::ReaderBuilder;
use csv_index::RandomAccessSimple;

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

// A BOM at the start of the file is stripped and excluded from byte offsets;
// a BOM-like sequence at the start of a later record is ordinary data that
// must survive indexing and random access.
#[test]
fn bom_start_and_later_record() {
    let mut data = BOM.to_vec();
    data.extend_from_slice(b"h1,h2\na,b\n");
    data.extend_from_slice(BOM);
    data.extend_from_slice(b"v1,v2\n");
    let content = data.clone();

    // Build and reopen the index.
    let mut rdr =
        ReaderBuilder::new().from_reader(io::Cursor::new(content.clone()));
    let mut idxbuf = io::Cursor::new(vec![]);
    RandomAccessSimple::create(&mut rdr, &mut idxbuf).unwrap();
    let mut idx = RandomAccessSimple::open(idxbuf).unwrap();
    assert_eq!(idx.len(), 3);

    // The first record starts at 0; later offsets count the BOM's bytes.
    assert_eq!(idx.get(0).unwrap().byte(), 0);
    // BOM(3) + "h1,h2\n"(6) = 9
    assert_eq!(idx.get(1).unwrap().byte(), 9);
    // 9 + "a,b\n"(4) = 13
    assert_eq!(idx.get(2).unwrap().byte(), 13);

    // Sequential read for reference (index includes the header row).
    let mut seq = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(io::Cursor::new(content.clone()));
    let s0 = seq.records().next().unwrap().unwrap();
    let s1 = seq.records().next().unwrap().unwrap();
    let s2 = seq.records().next().unwrap().unwrap();
    assert_eq!(s0, vec!["h1", "h2"]);
    assert_eq!(s1, vec!["a", "b"]);
    assert_eq!(s2, vec!["\u{feff}v1", "v2"]);

    // Random access must match the sequential read.
    let mut rdr =
        ReaderBuilder::new().from_reader(io::Cursor::new(content.clone()));
    for (i, expected) in [s0, s1, s2].iter().enumerate() {
        let pos = idx.get(i as u64).unwrap();
        rdr.seek(pos).unwrap();
        let got = rdr.records().next().unwrap().unwrap();
        assert_eq!(&got, expected, "record {}", i);
    }
}

// A file containing only the BOM indexes zero records.
#[test]
fn bom_only_is_empty() {
    let mut rdr =
        ReaderBuilder::new().from_reader(io::Cursor::new(BOM.to_vec()));
    let mut idxbuf = io::Cursor::new(vec![]);
    RandomAccessSimple::create(&mut rdr, &mut idxbuf).unwrap();
    let idx = RandomAccessSimple::open(idxbuf).unwrap();
    assert_eq!(idx.len(), 0);
}
