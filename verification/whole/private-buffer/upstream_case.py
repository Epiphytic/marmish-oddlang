"""The unchanged upstream reader test and its private test-only imports."""
import hashlib
import subprocess


TEST_HASH = "f841185690a16f313b54410278ed093e80aa9078dd790c5090d698d7ea763fcc"


def original_test(original, pin, source):
    data = subprocess.check_output(["git","-C",original,"show",f"{pin}:{source}"])
    marker = b"    #[test]\n    fn private_reader_wipes_consumed_spans"
    if data.count(marker) != 1:
        raise ValueError("unexpected original test")
    start = data.index(marker)
    body = data[start:data.index(b"    #[test]",start+len(marker))]
    if hashlib.sha256(body).hexdigest() != TEST_HASH:
        raise ValueError("upstream reader test changed")
    return (b"\n#[cfg(test)]\nmod secret_request_tests {\nuse super::*;\nuse std::io::{self, BufRead};\n"
            b"use zeroize::Zeroizing;\n"+body+b"}\n")
