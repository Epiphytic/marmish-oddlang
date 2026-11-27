use public_data_qualification::{
    Result,
    status_package::{Profile, execute},
};

fn main() -> Result<()> {
    execute(Profile::Typed)
}
