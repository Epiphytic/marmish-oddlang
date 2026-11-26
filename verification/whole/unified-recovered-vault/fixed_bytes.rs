use marmish_vault::Secret;

#[test]
fn from_bytes_owns_the_input_and_requires_drop() {
    let expected: [u8; 32] = std::array::from_fn(|index| index as u8);
    let mut caller = expected;
    let value = Secret::from_bytes(caller);
    caller.fill(0xff);

    let borrowed: &[u8; 32] = value.as_bytes();
    assert_eq!(borrowed, &expected);
    assert_ne!(borrowed.as_ptr(), caller.as_ptr());
    assert!(std::mem::needs_drop::<Secret>());

    drop(value);
    assert_eq!(caller, [0xff; 32]);
}
