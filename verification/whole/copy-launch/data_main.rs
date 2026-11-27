use copy_manifest_replay::launch_data::{constructs, decodes};

fn main() -> serde_json::Result<()> {
    let (creates, expected) = constructs();
    let corpus = serde_json::json!({"creates":creates,"expected":expected,"decodes":decodes()});
    serde_json::to_writer(std::io::stdout().lock(), &corpus)
}
