use public_data_qualification::{
    Result,
    owner_projection::{Owner, execute},
};
fn main() -> Result<()> {
    execute(Owner::Foreground)
}
