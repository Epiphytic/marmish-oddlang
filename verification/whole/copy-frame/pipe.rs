//! Local byte-stream witness, compiled separately against each marmish-stream library.
use marmish_stream::{MAX_BLOCK, copy_protocol::Frame};
use std::io::{self, Read, Write};

fn encode(input: &mut impl Read, output: &mut impl Write) -> io::Result<()> {
    let mut offset = 0;
    loop {
        for size in [1, 7, 257, MAX_BLOCK - 1, MAX_BLOCK] {
            let mut bytes = Vec::new();
            input.take(size as u64).read_to_end(&mut bytes)?;
            if bytes.is_empty() {
                return Ok(());
            }
            let len = bytes.len();
            let frame = Frame::Data { offset, bytes };
            let wire = frame
                .encode()
                .map_err(|e| io::Error::other(format!("{e:?}")))?;
            output.write_all(&wire)?;
            offset += len as u64;
        }
    }
}

fn decode(input: &mut impl Read, output: &mut impl Write) -> io::Result<()> {
    let mut offset = 0;
    loop {
        let mut header = [0; 5];
        if input.read(&mut header[..1])? == 0 {
            return Ok(());
        }
        input.read_exact(&mut header[1..])?;
        let len = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
        if len > MAX_BLOCK + 8 {
            return Err(io::Error::other("oversized Data frame"));
        }
        let mut record = header.to_vec();
        record.resize(header.len() + len, 0);
        input.read_exact(&mut record[header.len()..])?;
        let frame = Frame::decode(&record).map_err(|e| io::Error::other(format!("{e:?}")))?;
        match frame {
            Frame::Data {
                offset: start,
                bytes,
            } if start == offset => {
                output.write_all(&bytes)?;
                offset += bytes.len() as u64;
            }
            _ => return Err(io::Error::other("expected contiguous Data frames")),
        }
    }
}
fn main() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::BufWriter::new(io::stdout().lock());
    match std::env::args().nth(1).as_deref() {
        Some("encode") => encode(&mut input, &mut output)?,
        Some("decode") => decode(&mut input, &mut output)?,
        _ => return Err(io::Error::other("expected encode or decode")),
    }
    output.flush()
}
