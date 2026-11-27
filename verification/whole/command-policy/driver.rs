use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4);
        let executable = hex::decode(fields[1]).unwrap();
        let alias = (fields[2] != "-").then(|| hex::decode(fields[2]).unwrap());
        println!(
            "{}",
            policy_observation(
                fields[3].as_bytes(),
                [fields[0].parse().unwrap(); 32],
                &executable,
                alias.as_deref()
            )
        );
    }
}
