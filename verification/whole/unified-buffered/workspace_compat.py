"""Keep the buffered workspace's callbacks available to its downstream releases."""
from pathlib import Path
import runpy

HERE = Path(__file__).resolve().parent
PRIVATE = HERE.parent / "private-buffer"
SIX = runpy.run_path(str(HERE.parent / "unified-mdk/workspace_compat.py"))
WORKSPACE = "workspace-v7-recovered-vault-engine-journal-mdk-buffered.json"
SOURCE = "crates/marmish-prototype/src/lib.rs"


def check_product(product):
    return SIX["product_witness"]("buffered-product", product)


def buffer_oracle(product, original, host):
    result = SIX["product_witness"]("buffered-oracle", product, original, host)
    return {"bufferedInput": result}


def successor_reader(product, original, host, prior):
    return SIX["product_witness"]("buffered-oracle", product, original, host, "--prior", prior)
