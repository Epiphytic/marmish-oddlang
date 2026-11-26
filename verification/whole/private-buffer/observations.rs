#[cfg(test)]
mod buffer_contract_observations {
    use super::PrivateReader;
    use std::collections::VecDeque;
    use std::io::{self, BufRead, Read};
    use zeroize::Zeroizing;

    struct Steps {
        inputs: VecDeque<io::Result<Vec<u8>>>,
        calls: usize,
    }

    impl Read for Steps {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            assert_eq!(out.len(), 8192);
            let bytes = self.inputs.pop_front().expect("unexpected extra upstream read")?;
            out[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
    }

    fn reader(inputs: Vec<io::Result<Vec<u8>>>) -> PrivateReader<Steps> {
        PrivateReader {
            inner: Steps { inputs: inputs.into(), calls: 0 },
            bytes: Zeroizing::new([0; 8192]),
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn retained_span_and_consumed_bytes() {
        let mut r = reader(vec![Ok(vec![3, 4, 5]), Ok(vec![])]);
        let mut out = [99; 2];
        assert_eq!(r.read(&mut out).unwrap(), 2);
        assert_eq!(out, [3, 4]);
        assert_eq!(&r.bytes[..3], &[0, 0, 5], "consumed span must be erased");
        assert_eq!(r.fill_buf().unwrap(), &[5]);
        r.consume(0);
        assert_eq!(r.fill_buf().unwrap(), &[5]);
        r.consume(usize::MAX);
        assert!(r.bytes.iter().all(|byte| *byte == 0));
        assert_eq!(r.inner.calls, 1);
        assert!(r.fill_buf().unwrap().is_empty());
        assert_eq!(r.inner.calls, 2);
    }

    #[test]
    fn failed_refill_retains_prior_end() {
        let mut r = reader(vec![Ok(vec![1, 2]), Err(io::ErrorKind::Interrupted.into()), Ok(vec![9])]);
        assert_eq!(r.fill_buf().unwrap(), &[1, 2]);
        r.consume(2);
        assert_eq!(r.fill_buf().unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!((r.start, r.end), (0, 2), "failed refill preserves old end");
        assert_eq!(r.fill_buf().unwrap(), &[0, 0]);
        assert_eq!(r.inner.calls, 2);
        r.consume(2);
        assert_eq!(r.fill_buf().unwrap(), &[9]);
        assert_eq!(r.inner.calls, 3);
    }

    #[test]
    fn zero_output_still_reads_and_eof_can_be_polled_again() {
        let mut r = reader(vec![Ok(vec![]), Ok(vec![7])]);
        assert_eq!(r.read(&mut []).unwrap(), 0);
        assert_eq!(r.inner.calls, 1, "empty destination must still poll exhausted input");
        assert_eq!(r.read(&mut []).unwrap(), 0);
        assert_eq!(r.inner.calls, 2);
        assert_eq!(r.fill_buf().unwrap(), &[7]);
    }
}
