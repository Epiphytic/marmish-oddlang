use core_qualification::{Result, comparison::compare, pretty};

fn main() -> Result {
    println!("{}", pretty(&compare()?)?);
    Ok(())
}
