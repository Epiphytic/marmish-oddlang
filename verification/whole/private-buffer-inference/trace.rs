#[cfg(test)]
mod portable_buffer_behavior {
    use super::PrivateReader;
    use std::collections::VecDeque;
    use std::io::{self, BufRead, Read};
    use zeroize::Zeroizing;

    enum Step { Bytes(Vec<u8>), Interrupt, End }

    struct Input {
        steps: VecDeque<Step>,
        calls: usize,
        observed_prefix: Vec<u8>,
        prefix_len: usize,
    }

    impl Read for Input {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            match self.steps.pop_front().expect("unexpected input poll") {
                Step::Bytes(bytes) => {
                    out[..bytes.len()].copy_from_slice(&bytes);
                    Ok(bytes.len())
                }
                Step::Interrupt => Err(io::ErrorKind::Interrupted.into()),
                Step::End => {
                    self.observed_prefix = out[..self.prefix_len].to_vec();
                    Ok(0)
                }
            }
        }
    }

    fn reader(payload: &[u8], tail: impl IntoIterator<Item=Step>) -> PrivateReader<Input> {
        let steps = std::iter::once(Step::Bytes(payload.to_vec())).chain(tail).collect();
        PrivateReader {
            inner: Input { steps, calls: 0, observed_prefix: vec![], prefix_len: payload.len() },
            bytes: Zeroizing::new([0; 8192]),
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn collect() {
        let p = [3, 5, 7];
        let mut empty = reader(&p, []);
        let returned = empty.read(&mut []).unwrap();
        let empty_calls = empty.inner.calls;
        let view = empty.fill_buf().unwrap().to_vec();
        println!("emptyRead {{\"payload\":{p:?},\"returned\":{returned},\"callsAfterEmpty\":{empty_calls},\"afterFill\":{view:?},\"callsAfterFill\":{}}}",empty.inner.calls);

        let p = [11, 13, 17];
        let mut consumption = reader(&p, [Step::End]);
        assert_eq!(consumption.fill_buf().unwrap(), &p);
        consumption.consume(1);
        let suffix = consumption.fill_buf().unwrap().to_vec();
        let partial_calls = consumption.inner.calls;
        consumption.consume(p.len()-1);
        let eof = consumption.fill_buf().unwrap().to_vec();
        println!("consume {{\"payload\":{p:?},\"requested\":1,\"afterPartial\":{suffix:?},\"callsAfterPartial\":{partial_calls},\"nextReadPrefix\":{:?},\"afterRefill\":{eof:?},\"callsAfterRefill\":{}}}",consumption.inner.observed_prefix,consumption.inner.calls);

        let p = [19, 23, 29];
        let mut failed = reader(&p,[Step::Interrupt,Step::End]);
        assert_eq!(failed.fill_buf().unwrap(), &p);
        failed.consume(p.len());
        let interrupted = matches!(failed.fill_buf(),Err(ref error) if error.kind()==io::ErrorKind::Interrupted);
        let error_calls = failed.inner.calls;
        let retry = failed.fill_buf().unwrap().to_vec();
        println!("refillError {{\"payload\":{p:?},\"returnedError\":{interrupted},\"callsAfterError\":{error_calls},\"afterRetry\":{retry:?},\"callsAfterRetry\":{}}}",failed.inner.calls);
    }
}
