pub fn policy_observation(
    raw: &[u8],
    who: [u8; 32],
    executable: &[u8],
    alias: Option<&[u8]>,
) -> String {
    let decision = CommandRestrictions::from_json(raw).and_then(|config| {
        config.validate_for_principals(&[[0; 32], [1; 32]])?;
        config.check_named_exec(who, executable, alias)
    });
    match decision {
        Ok(value) => value.to_string(),
        Err(error) => error.to_owned(),
    }
}

pub fn observe_room_config(root: &Path, endpoint: &str) -> Result<bool, Failure> {
    let config = room_config(root, endpoint)?;
    match config
        .command_policy
        .check_named_exec([0; 32], b"/usr/bin/echo", None)
    {
        Ok(true) => Ok(true),
        Err("command_denied") => Ok(false),
        other => panic!("unexpected policy observation {other:?}"),
    }
}
