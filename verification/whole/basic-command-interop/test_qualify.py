import unittest
from dataclasses import replace

from qualify import Output, detached_handle, finite_goodput


class ComparisonTests(unittest.TestCase):
    def test_streams_and_status_are_independent(self):
        reference = Output(7, b"\xff\n\n", b"\0")
        reference.compare(reference)
        for change in (dict(status=0), dict(stdout=b"\xff\n"), dict(stderr=b""),
                       dict(stdout=reference.stderr, stderr=reference.stdout)):
            with self.subTest(change=change), self.assertRaises(ValueError):
                replace(reference, **change).compare(reference)

    def test_detach_format_and_receiver(self):
        receiver = "1a" * 32
        encoded = ('{"format":1,"profile":"client","remote":"' + receiver
                   + '","session":"' + "03" * 16 + '","invocation":"' + "04" * 16
                   + '","submit_sha256":"' + "ff" * 32 + '"}').encode()
        output = Output(0, b"", b"marmish handle: " + encoded + b"\n")
        self.assertEqual(detached_handle(output, receiver), encoded)
        for change in (dict(stderr=output.stderr.rstrip()), dict(status=7), dict(stdout=b"\n"),
                       dict(stderr=output.stderr.replace(b'":1', b'": 1')),
                       dict(stderr=output.stderr + b"extra\n")):
            with self.subTest(change=change), self.assertRaises(ValueError):
                detached_handle(replace(output, **change), receiver)
        with self.assertRaises(ValueError):
            detached_handle(output, "ff" * 32)

    def test_goodput_requires_exact_data_and_wall_time(self):
        output = Output(7, b"a" * 196608, b"b" * 65536)
        self.assertEqual(finite_goodput(output, output, 2_000_000_000), 131072)
        for ns in (0, -1, 330_000_000_001, True):
            with self.subTest(ns=ns), self.assertRaises(ValueError):
                finite_goodput(output, output, ns)
        with self.assertRaises(ValueError):
            finite_goodput(output, replace(output, stdout=b"c" * 196608), 1)
        with self.assertRaises(ValueError):
            finite_goodput(Output(0, b"", b""), Output(0, b"", b""), 1)
