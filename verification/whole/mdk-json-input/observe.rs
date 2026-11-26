#[cfg(test)]
mod bounded_metadata_observation {
    use super::{parse_json, parse_ndjson};
    use crate::AdapterError;
    use std::io::{self, BufRead, BufReader, Cursor, Read};

    fn boundary(range: u32, accepts: impl Fn(u32) -> bool) -> (u32, u32) {
        let (mut low, mut high) = (2, range);
        assert!(accepts(low));
        assert!(!accepts(high));
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            if accepts(mid) {
                low = mid;
            } else {
                high = mid;
            }
        }
        (low, high)
    }

    fn padded(n: usize, delimiter: bool) -> Vec<u8> {
        let mut data = vec![b' '; n];
        data[..2].copy_from_slice(b"{}");
        if delimiter {
            data[n - 1] = b'\n';
        }
        data
    }

    struct BadReader;
    impl Read for BadReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("in-memory witness"))
        }
    }
    impl BufRead for BadReader {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("in-memory witness"))
        }
        fn consume(&mut self, _: usize) {
            panic!("nothing was supplied");
        }
    }

    #[test]
    fn discover_bounds() {
        let (bytes, bytes_over) = boundary(4_194_304, |n| match parse_json(&padded(n as usize, false)) {
            Ok(_) => true,
            Err(AdapterError::OutputLimit) => false,
            other => panic!("unexpected byte response: {other:?}"),
        });
        let (records, records_over) = boundary(4096, |n| match parse_ndjson(&b"{}\n".repeat(n as usize)[..]) {
            Ok(values) => values.len() == n as usize,
            Err(AdapterError::OutputLimit) => false,
            other => panic!("unexpected count response: {other:?}"),
        });
        let line = padded(bytes as usize, true);
        assert_eq!(parse_ndjson(BufReader::with_capacity(7, &line[..])).unwrap().len(), 1);
        assert_eq!(parse_ndjson(&line.repeat(2)[..]).unwrap().len(), 2);
        assert!(matches!(
            parse_ndjson(BufReader::with_capacity(7, &padded(bytes_over as usize, true)[..])),
            Err(AdapterError::OutputLimit)
        ));
        assert!(matches!(parse_json(&vec![b'x'; bytes_over as usize]), Err(AdapterError::OutputLimit)));
        assert!(matches!(parse_json(b"{"), Err(AdapterError::InvalidResponse)));
        assert!(matches!(parse_ndjson(&b"{}\n{\n"[..]), Err(AdapterError::InvalidResponse)));
        assert!(matches!(parse_ndjson(&b"{}"[..]), Err(AdapterError::InvalidResponse)));
        assert!(matches!(parse_ndjson(BadReader), Err(AdapterError::InvalidResponse)));
        assert!(parse_ndjson(&b""[..]).unwrap().is_empty());
        assert_eq!(parse_json(br#"{"x":0,"x":7}"#).unwrap()["x"], 7);

        let mut at_count = b"{}\n".repeat(records as usize);
        let prefix = at_count.len();
        at_count.extend(padded(bytes_over as usize, true));
        let mut oversized = Cursor::new(at_count);
        assert!(matches!(parse_ndjson(&mut oversized), Err(AdapterError::OutputLimit)));
        assert_eq!(oversized.position(), prefix as u64);

        let mut malformed = b"{}\n".repeat(records as usize);
        malformed.extend_from_slice(b"{\n");
        let mut next = Cursor::new(malformed);
        assert!(matches!(parse_ndjson(&mut next), Err(AdapterError::OutputLimit)));
        assert_eq!(next.position(), (prefix + 2) as u64);
        println!("BOUNDED_METADATA={}", serde_json::json!({
            "format": "oddlang-bounded-json-observation-v1",
            "acceptedValueBytes": bytes, "rejectedValueBytes": bytes_over,
            "acceptedRecords": records, "rejectedRecords": records_over,
            "cases": {
                "byte-before-count": "capacity-unconsumed",
                "count-before-json": "capacity-consumed",
                "duplicate-member": "last-value",
                "empty-sequence": "empty",
                "malformed-json": "invalid",
                "malformed-after-line": "invalid",
                "newline-counts": "capacity",
                "per-record-budget": "accepted",
                "read-error": "invalid",
                "terminal-line": "invalid",
                "value-size-before-json": "capacity",
            },
        }));
    }
}
