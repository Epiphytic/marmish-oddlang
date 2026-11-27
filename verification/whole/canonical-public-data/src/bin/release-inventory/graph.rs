use super::*;
use std::{collections::BTreeSet, fmt::Write};
fn quote(value: &str) -> String {
    let mut result = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            result.push(char::from(b));
        } else {
            write!(result, "%{b:02X}").unwrap();
        }
    }
    result
}
struct Graph<'a> {
    namespace: &'a str,
    rows: BTreeSet<String>,
}
impl Graph<'_> {
    fn node(&self, path: &[&str]) -> String {
        format!(
            "<{}{}>",
            self.namespace,
            path.iter().map(|p| quote(p)).collect::<Vec<_>>().join(":")
        )
    }
    fn fact(&mut self, s: &str, p: &str, o: &str) {
        self.rows.insert(format!("{s} {p} {o} ."));
    }
    fn literal(&mut self, s: &str, p: &str, value: &str) {
        self.fact(s, p, &json!(value).to_string());
    }
    fn identity(&mut self, s: &str, class: &str, label: &str) {
        self.fact(s, "a", &format!("i:{class}"));
        self.literal(s, "rdfs:label", label);
    }
    fn obligation(&mut self, s: &str, role: &str, check: &str) {
        let id = format!(
            "{}:obligation:{}>",
            s.strip_suffix('>').unwrap(),
            quote(role)
        );
        self.identity(&id, "Obligation", check);
        self.fact(&id, "i:appliesTo", s);
        self.literal(&id, "i:role", role);
    }
}
pub fn project(spec: &Value, data: &Value) -> Result<String> {
    let mut g = Graph {
        namespace: string(&spec["namespace"])?,
        rows: BTreeSet::new(),
    };
    let commit = string(&data["commit"])?;
    let release = g.node(&["release", commit]);
    g.identity(&release, "Release", commit);
    for w in list(&data["workspaces"])? {
        for p in list(&w["packages"])? {
            let manifest = string(&p["manifest"])?;
            let owner = g.node(&["package", manifest]);
            g.identity(&owner, "Package", string(&p["name"])?);
            g.fact(&release, "i:contains", &owner);
            for name in object(&p["features"])?.keys() {
                let id = g.node(&["feature", manifest, name]);
                g.identity(&id, "Feature", name);
                g.fact(&owner, "i:declares", &id);
            }
            for t in list(&p["targets"])? {
                let kind = strings(&t["kind"])?.join("+");
                let name = string(&t["name"])?;
                let id = g.node(&["target", manifest, &kind, name]);
                g.identity(&id, "CargoTarget", name);
                g.fact(&owner, "i:declares", &id);
                g.literal(&id, "i:kind", &kind);
                for f in strings(&t["required_features"])? {
                    g.fact(&id, "i:requires", &g.node(&["feature", manifest, f]));
                }
                let (role, check) = match kind.as_str() {
                    "lib" => (
                        "consumer",
                        "Compile unchanged consumers and compare behavior",
                    ),
                    "bin" => ("process", "Compare process I/O, status and effects"),
                    _ => ("build", "Build and exercise the selected target"),
                };
                g.obligation(&id, role, check);
                for (key, description) in [
                    ("test", "Run original Cargo target test harness"),
                    ("doctest", "Run unchanged documentation tests"),
                ] {
                    if t[key].as_bool().ok_or("expected target flag")? {
                        g.obligation(&id, key, description);
                    }
                }
            }
        }
    }
    for s in list(&spec["surfaces"])? {
        let name = string(&s["id"])?;
        let family = g.node(&["surface", name]);
        g.identity(&family, "Surface", string(&s["contract"])?);
        g.literal(&family, "i:owner", string(&s["owner"])?);
        g.fact(&release, "i:contains", &family);
        for check in strings(&s["checks"])? {
            g.literal(&family, "i:check", check);
        }
        for path in strings(&data["file_groups"][name])? {
            let id = g.node(&["file", path]);
            g.identity(&id, "Input", path);
            g.fact(&family, "i:contains", &id);
        }
    }
    for name in strings(&data["release_assets"])? {
        let id = g.node(&["asset", name]);
        g.identity(&id, "DistributionAsset", name);
        g.fact(&release, "i:ships", &id);
        g.obligation(
            &id,
            "distribution",
            "Validate staged asset against upstream release requirements",
        );
    }
    for cell in list(&data["baseline_cells"])? {
        let target = string(&cell["target"])?;
        let profile = string(&cell["profile"])?;
        let id = g.node(&["cell", target, profile]);
        g.identity(&id, "BaselineCell", &format!("{target} {profile}"));
        g.fact(&release, "i:requires", &id);
        g.obligation(
            &id,
            "original",
            "Capture repeatable reference outcomes on a matching host",
        );
    }
    for p in list(&spec["release"]["external_programs"])? {
        let name = string(&p["name"])?;
        let id = g.node(&["program", name]);
        g.identity(&id, "ExternalProgram", name);
        g.fact(&release, "i:references", &id);
        for key in ["scope", "witness"] {
            g.literal(&id, &format!("i:{key}"), string(&p[key])?);
        }
        for key in ["source", "revision", "lock_sha256"] {
            if let Some(v) = p.get(key) {
                g.literal(&id, &format!("i:{key}"), string(v)?);
            }
        }
    }
    let prefixes = "@prefix i: <urn:oddlang:inventory:> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";
    Ok(format!(
        "{prefixes}\n{}\n",
        g.rows.into_iter().collect::<Vec<_>>().join("\n")
    ))
}
