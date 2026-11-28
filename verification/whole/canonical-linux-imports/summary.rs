//! Compare the frozen public import inventory across original Linux and both generated hosts.
use public_data_qualification::{Result, imports::COUNT, list, require};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const ROLES: [&str; 3] = ["originalLinux", "generatedLinux", "generatedDarwin"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Import<'a> {
    spelling: &'a str,
    original_inventory_kind: &'a str,
    compiles: bool,
    #[serde(borrow)]
    diagnostics: Vec<Diagnostic<'a>>,
}

#[derive(Deserialize)]
struct Diagnostic<'a> {
    code: &'a str,
    message: &'a str,
}

fn counts(attempted: usize, passed: [usize; 3]) -> Value {
    json!({"attempted":attempted, "originalLinux":passed[0], "generatedLinux":passed[1], "generatedDarwin":passed[2]})
}

pub fn compare(data: &Value) -> Result<Value> {
    let mut by_library = json!({});
    let mut total = [0; 3];
    let mut outcomes = Vec::new();
    let mut diagnostics = Vec::new();
    let mut codes = ROLES.map(|_| BTreeMap::<&str, usize>::new());
    for (library, count) in COUNT {
        let mut passed = [0; 3];
        let mut seen = BTreeSet::new();
        let rows = ROLES.map(|role| list(&data[role][library]["imports"]));
        let [original, generated, darwin] = rows;
        let rows = [original?, generated?, darwin?];
        require(
            rows.iter().all(|r| r.len() == count),
            "unexpected public import count",
        )?;
        for i in 0..count {
            let [a, b, c] = rows.map(|r| Import::deserialize(&r[i]));
            let cases = [a?, b?, c?];
            let spelling = cases[0].spelling;
            require(
                seen.insert(spelling)
                    && cases.iter().all(|case| {
                        case.spelling == spelling
                            && case.original_inventory_kind == cases[0].original_inventory_kind
                    }),
                "public import identity changed",
            )?;
            for (role, case) in cases.iter().enumerate() {
                passed[role] += usize::from(case.compiles);
                for diagnostic in &case.diagnostics {
                    *codes[role].entry(diagnostic.code).or_default() += 1;
                }
            }
            let [a, b, c] = cases;
            if !a.compiles || b.compiles != c.compiles {
                outcomes.push(json!({"library":library,"spelling":spelling,"originalLinux":a.compiles,"generatedLinux":b.compiles,"generatedDarwin":c.compiles,"originalDarwin":true}));
            }
            let messages = |case: &Import| -> Vec<Value> {
                case.diagnostics
                    .iter()
                    .map(|d| json!([d.code, d.message]))
                    .collect()
            };
            let linux = messages(&b);
            let mac = messages(&c);
            if linux != mac {
                diagnostics.push(json!({"library":library,"spelling":spelling,"generatedLinux":linux,"generatedDarwin":mac}));
            }
        }
        if library == "core" {
            require(
                passed[0] == count && passed[1] == count,
                "core Linux imports regressed",
            )?;
        }
        by_library[library] = counts(count, passed);
        for (sum, n) in total.iter_mut().zip(passed) {
            *sum += n;
        }
    }
    let rejections: BTreeMap<_, _> = ROLES.into_iter().zip(codes).collect();
    Ok(
        json!({"counts":by_library,"totals":counts(COUNT.iter().map(|(_,n)| n).sum(), total),"platformOutcomeDifferences":outcomes,"generatedDiagnosticDifferences":diagnostics,"rejectionDiagnosticsByCode":rejections}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use public_data_qualification::{load, whole};

    #[test]
    fn reproduces_the_published_cross_platform_summary() -> Result {
        let linux = load(&whole().join("canonical-linux-imports/aarch64-unknown-linux-gnu.json"))?;
        let darwin = load(&whole().join("canonical-workspace/aarch64-apple-darwin.json"))?;
        let mut input = json!({
            "originalLinux": linux["originalBuild"]["libraries"],
            "generatedLinux": linux["generatedBuild"]["libraries"],
            "generatedDarwin": darwin["darwinPublicImports"]["libraries"]
        });
        let report = compare(&input)?;
        for (field, value) in report.as_object().unwrap() {
            assert_eq!(value, &linux[field], "{field}");
        }
        input["generatedDarwin"]["engine"]["imports"][0]["spelling"] = "changed".into();
        assert!(compare(&input).is_err());
        Ok(())
    }
}
