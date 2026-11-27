// Appended inside the pinned command_config module to access its private normalizer.
pub fn policy_observation(
    raw: &[u8],
    who: [u8; 32],
    executable: &[u8],
    alias: Option<&[u8]>,
) -> String {
    use marmish_engine::{
        CommandRequest, EngineError, EngineLimits, EnginePolicy, PrincipalId, ServerId,
    };
    let admitted = [PrincipalId([0; 32]), PrincipalId([1; 32])];
    let mut config = br#"{"version":1,"role":"server","relays":["wss://relay.example/"],"tor":"auto","trusted_remotes":[],"execution_clients":[],"cwd":"/","limits":{},"aliases":{},"command_policy":"#.to_vec();
    config.extend_from_slice(raw);
    config.push(b'}');
    let command_policy =
        match CommandConfig::from_bytes(&config).and_then(|config| config.command_policy()) {
            Ok(p) => p,
            Err(Error::Invalid(message)) => return message.into(),
            Err(other) => panic!("unexpected config error: {other:?}"),
        };
    if command_policy.validate_for_principals(&admitted).is_err() {
        return "invalid_policy".into();
    }
    let policy = EnginePolicy {
        state_root: PathBuf::new(),
        worker_executable: PathBuf::new(),
        server: ServerId([0; 32]),
        allowed_principals: admitted.to_vec(),
        allow_shell_command: false,
        cwd: None,
        command_policy: Some(command_policy),
        limits: EngineLimits::default(),
    };
    let argv = vec![executable.to_vec()];
    let request = match alias {
        Some(path) => CommandRequest::ResolvedExec {
            executable: executable.to_vec(),
            invoked_path: path.to_vec(),
            argv,
        },
        None => CommandRequest::Exec { argv },
    };
    match policy.check_exec_restrictions(PrincipalId(who), &request) {
        Ok(value) => value.to_string(),
        Err(EngineError::CommandDenied) => "command_denied".into(),
        Err(EngineError::InvalidRequest) => "invalid_request".into(),
        Err(EngineError::InvalidPolicy) => "invalid_policy".into(),
        Err(other) => panic!("unexpected decision error: {other:?}"),
    }
}
