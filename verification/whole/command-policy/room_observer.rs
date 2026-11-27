fn main() {
    let root = std::env::args_os().nth(1).unwrap();
    let result =
        marmish_native::observe_room_config(std::path::Path::new(&root), "wss://relay.example/");
    match result {
        Ok(true) => println!("loaded-allow"),
        Ok(false) => println!("loaded-deny"),
        Err(error) => println!("{error}"),
    }
}
