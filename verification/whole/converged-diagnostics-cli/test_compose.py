import unittest

from compose import combine_packages


def package(*components, version="1.0.0"):
    return {"version": version, "components": [
        {"kind": kind, "module": module, "graph": graph}
        for kind, module, graph in components
    ]}


class ComponentComposition(unittest.TestCase):
    def test_disjoint_and_identical_changes(self):
        base = package(("parse", "args", "base"), ("types", "reports", "existing"))
        cli = package(("parse", "args", "updated"), ("types", "reports", "existing"))
        diagnostic = package(("parse", "args", "updated"), ("types", "reports", "existing"),
                             ("state", "diagnostics", "added"))
        self.assertEqual(combine_packages(base, [cli, diagnostic]), diagnostic)
        self.assertEqual(combine_packages(base, [diagnostic, cli]), diagnostic)

    def test_overlapping_change_is_rejected(self):
        base = package(("parse", "args", "base"))
        left = package(("parse", "args", "left"))
        right = package(("parse", "args", "right"))
        with self.assertRaisesRegex(ValueError, "incompatible edits to native component"):
            combine_packages(base, [left, right])

    def test_removed_or_duplicate_component_is_rejected(self):
        base = package(("parse", "args", "base"))
        with self.assertRaisesRegex(ValueError, "lost inherited components"):
            combine_packages(base, [package()])
        with self.assertRaisesRegex(ValueError, "duplicate native component"):
            combine_packages(base, [package(("parse", "args", "left"), ("parse", "args", "right"))])

    def test_metadata_change_is_rejected(self):
        base = package(("parse", "args", "base"))
        with self.assertRaisesRegex(ValueError, "package metadata differs"):
            combine_packages(base, [package(("parse", "args", "base"), version="2.0.0")])


if __name__ == "__main__":
    unittest.main()
