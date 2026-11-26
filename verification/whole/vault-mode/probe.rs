use marmish_vault::helper::AuthorizationMode;

fn observe<T: Clone + Copy>(value: T) -> T { value.clone() }

fn main() {
    for value in [AuthorizationMode::Interactive, AuthorizationMode::Verify] {
        let label = match observe(value) {
            AuthorizationMode::Interactive => "interactive",
            AuthorizationMode::Verify => "verify",
        };
        println!("{label}={}", value as isize);
    }
}
