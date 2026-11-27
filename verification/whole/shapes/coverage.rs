//! Independently check v1 declaration identities, structural types and reported gaps.
use public_data_qualification::{Result, list, require, text};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const ROOT: &[&str] = &[
    "data-layout-and-representation",
    "visibility-and-export-paths",
    "workspace-package-identities",
];
const UNRESOLVED: &[&str] = &[
    "array-length",
    "constant-argument",
    "dynamic-type",
    "function-pointer",
    "lifetime-binding",
    "type-alias-projection",
    "unsupported-type",
];
const OTHER: &[&str] = &[
    "compiler-disambiguated-identity",
    "explicit-discriminant",
    "external-package-instance",
    "generic-constraints-and-defaults",
    "raw-pointer-provenance",
    "union-active-field",
];

fn keys(row: &Value, required: &str, optional: Option<&str>) -> Result<()> {
    let fields = row.as_object().ok_or("expected object")?;
    let mut count = 0;
    for key in required.split_ascii_whitespace() {
        require(
            fields.contains_key(key),
            format!("missing graph field {key}"),
        )?;
        count += 1;
    }
    if optional.is_some_and(|key| fields.contains_key(key)) {
        count += 1;
    }
    require(count == fields.len(), "unexpected graph fields")
}

fn ordered<'a>(row: &'a Value, kind: &str) -> Result<&'a [Value]> {
    let rows = list(row)?;
    require(
        rows.iter()
            .enumerate()
            .all(|(i, v)| v["position"].as_u64() == Some(i as u64)),
        format!("invalid {kind} order"),
    )?;
    Ok(rows)
}
type Gap<'a> = (&'a str, &'a str);
struct Coverage<'a> {
    prefix: String,
    identities: BTreeSet<&'a str>,
    local_references: BTreeSet<&'a str>,
    required: BTreeSet<Gap<'a>>,
}

impl<'a> Coverage<'a> {
    fn register(&mut self, row: &'a Value, owner: Option<&str>) -> Result<&'a str> {
        let id = text(&row["id"])?;
        require(
            id.starts_with(&self.prefix),
            format!("foreign identity {id}"),
        )?;
        require(
            self.identities.insert(id),
            format!("duplicate identity {id}"),
        )?;
        if let Some(owner) = owner {
            require(row["owner"] == owner, format!("incorrect owner for {id}"))?;
        }
        Ok(id)
    }
    fn shape(&mut self, shape: &'a Value, owner: &'a str) -> Result<()> {
        let kind = text(&shape["kind"])?;
        let fields = match kind {
            "boolean" | "unicode-scalar" | "text" | "never" | "static-lifetime" => "kind",
            "signed-integer" | "unsigned-integer" | "floating-point" => "kind bits",
            "target-integer" => "kind bits signed",
            "named" => "kind id arguments",
            "parameter" => "kind id",
            "tuple" => "kind elements",
            "reference" => "kind ownership lifetime target",
            "raw-pointer" => "kind mutable target",
            "slice" => "kind element",
            "unresolved" => "kind reason",
            _ => return Err(format!("unknown shape kind {kind}").into()),
        };
        keys(shape, fields, (kind == "named").then_some("ownership"))?;
        match kind {
            "signed-integer" | "unsigned-integer" | "floating-point" | "target-integer" => {
                let bits = shape["bits"].as_u64();
                let valid = match kind {
                    "target-integer" => bits == Some(64),
                    "floating-point" => matches!(bits, Some(16 | 32 | 64 | 128)),
                    _ => matches!(bits, Some(8 | 16 | 32 | 64 | 128)),
                };
                require(valid, format!("invalid {kind} width at {owner}"))?;
            }
            "named" => {
                let id = text(&shape["id"])?;
                if id.starts_with("local::") {
                    self.local_references.insert(id);
                }
                for arg in ordered(&shape["arguments"], "type argument")? {
                    keys(arg, "name parameter position value", None)?;
                    self.shape(&arg["value"], owner)?;
                }
            }
            "tuple" => {
                for element in ordered(&shape["elements"], "tuple element")? {
                    keys(element, "position type", None)?;
                    self.shape(&element["type"], owner)?;
                }
            }
            "reference" => {
                self.shape(&shape["lifetime"], owner)?;
                self.shape(&shape["target"], owner)?;
            }
            "raw-pointer" => self.shape(&shape["target"], owner)?,
            "slice" => self.shape(&shape["element"], owner)?,
            "unresolved" => {
                let reason = text(&shape["reason"])?;
                require(
                    UNRESOLVED.contains(&reason),
                    format!("unknown unresolved reason {reason}"),
                )?;
                self.required.insert((owner, reason));
            }
            _ => {}
        }
        Ok(())
    }
    fn fields(&mut self, fields: &'a Value, owner: &str) -> Result<()> {
        for field in ordered(fields, "field")? {
            keys(field, "id owner name position type", None)?;
            let id = self.register(field, Some(owner))?;
            self.shape(&field["type"], id)?;
        }
        Ok(())
    }
    fn declaration(&mut self, decl: &'a Value) -> Result<&'a str> {
        let kind = text(&decl["kind"])?;
        let fields = match kind {
            "product" => "id name parameters kind form fields",
            "union" => "id name parameters kind fields",
            "sum" => "id name parameters kind variants",
            _ => return Err(format!("unknown declaration kind {kind}").into()),
        };
        keys(decl, fields, None)?;
        let id = self.register(decl, None)?;
        for param in ordered(&decl["parameters"], "parameter")? {
            keys(param, "id name position kind", None)?;
            self.register(param, None)?;
        }
        if kind == "sum" {
            for v in ordered(&decl["variants"], "variant")? {
                keys(v, "id owner name position form fields", None)?;
                let variant = self.register(v, Some(id))?;
                self.fields(&v["fields"], variant)?;
            }
        } else {
            self.fields(&decl["fields"], id)?;
        }
        Ok(id)
    }
}

pub fn coverage(graph: &Value, target: &str) -> Result<Value> {
    keys(graph, "vocabulary declarations gaps", None)?;
    require(
        graph["vocabulary"] == "oddlang-data-shapes-v1",
        "invalid vocabulary",
    )?;
    let root = format!("local::{}", target.replace('-', "_"));
    let mut state = Coverage {
        prefix: format!("{root}::"),
        identities: BTreeSet::from([root.as_str()]),
        local_references: BTreeSet::new(),
        required: ROOT.iter().map(|r| (root.as_str(), *r)).collect(),
    };
    let mut declarations = BTreeSet::new();
    for decl in list(&graph["declarations"])? {
        let id = state.declaration(decl)?;
        require(
            declarations.last().is_none_or(|last| *last < id),
            "invalid declaration order",
        )?;
        declarations.insert(id);
    }
    if let Some(id) = state.local_references.difference(&declarations).next() {
        return Err(format!("unknown local declaration {id}").into());
    }
    let mut observed = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for gap in list(&graph["gaps"])? {
        keys(gap, "owner reason", None)?;
        let owner = text(&gap["owner"])?;
        let reason = text(&gap["reason"])?;
        require(
            state.identities.contains(owner),
            format!("unknown gap owner {owner}"),
        )?;
        require(
            ROOT.contains(&reason) || UNRESOLVED.contains(&reason) || OTHER.contains(&reason),
            format!("unknown gap reason {reason}"),
        )?;
        require(
            !ROOT.contains(&reason) || owner == root,
            format!("invalid root gap owner {owner}"),
        )?;
        let pair = (owner, reason);
        require(
            !UNRESOLVED.contains(&reason) || state.required.contains(&pair),
            format!("unmatched unresolved gap at {owner}"),
        )?;
        require(
            observed.last().is_none_or(|last| *last < pair),
            "invalid gap order or duplicate gap",
        )?;
        observed.insert(pair);
        *counts.entry(reason).or_insert(0usize) += 1;
    }
    if let Some(gap) = state.required.difference(&observed).next() {
        return Err(format!("missing required gaps: {gap:?}").into());
    }
    Ok(json!({"declarations":declarations.len(),"gaps":counts}))
}
