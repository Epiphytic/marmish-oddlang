fn main() -> serde_json::Result<()> {
    serde_json::to_writer(
        std::io::stdout().lock(),
        &copy_manifest_replay::launch_hash::rows(),
    )
}
