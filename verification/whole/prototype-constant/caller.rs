use marmish_prototype::MAX_LINE;

const _: [(); 131_072] = [(); MAX_LINE];

fn main() {
    let limit: usize = MAX_LINE;
    println!("{limit}");
}
