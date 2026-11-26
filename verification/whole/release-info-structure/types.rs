// Appended temporarily to the scratch native crate for an independent type check.
const _: ::core::primitive::u8 = crate::identity::PROFILE_FORMAT;
const _: ::core::primitive::i64 = crate::journal::JOURNAL_FORMAT;
const _: ::core::primitive::i64 = ::marmish_engine::ENGINE_FORMAT;
const _: ::core::primitive::u32 = crate::command_config::CONFIG_FORMAT;
const _: ::core::primitive::u32 = crate::command_wire::COMMAND_FORMAT;

#[allow(dead_code)]
fn recovered_release_types() {
    let report = crate::native_manifest::compiled_release_metadata();
    let _: ::core::primitive::u8 = report.schema;
    let _: &'static ::core::primitive::str = report.version;
    let _: &'static ::core::primitive::str = report.target;
    let _: &'static ::core::primitive::str = report.commit;
    let _: [::core::primitive::u32; 5] = report.formats;
}
