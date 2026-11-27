use core_qualification::*;
use oxrdf::{Graph, NamedNode, NamedOrBlankNodeRef, TermRef, Triple};
use oxttl::TurtleParser;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::Path,
};

const NS: &str = "urn:oddlang:structural-ownership:";
const CORE: &str = "urn:oddlang:core:";
type Paths = BTreeMap<String, [Value; 2]>;
fn n(local: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{NS}{local}"))
}
fn rdf_type() -> NamedNode {
    oxrdf::vocab::rdf::TYPE.into_owned()
}
fn parse(file: &Path) -> Result<Graph> {
    Ok(TurtleParser::new()
        .for_reader(File::open(file)?)
        .collect::<std::result::Result<_, _>>()?)
}
fn named(t: TermRef) -> Result<NamedNode> {
    match t {
        TermRef::NamedNode(n) => Ok(n.into_owned()),
        _ => Err("expected an IRI".into()),
    }
}
fn subjects(g: &Graph, p: &NamedNode, o: &NamedNode) -> Result<BTreeSet<NamedNode>> {
    g.subjects_for_predicate_object(p, o)
        .map(|v| match v {
            NamedOrBlankNodeRef::NamedNode(n) => Ok(n.into_owned()),
            _ => Err("expected a named subject".into()),
        })
        .collect()
}
fn object_iri(g: &Graph, s: &NamedNode, p: &NamedNode) -> Result<NamedNode> {
    named(one(g.objects_for_subject_predicate(s, p))?)
}
fn literal(g: &Graph, s: &NamedNode, p: &NamedNode) -> Result<String> {
    match one(g.objects_for_subject_predicate(s, p))? {
        TermRef::Literal(l) => Ok(l.value().to_owned()),
        _ => Err("expected a literal".into()),
    }
}
fn inspect(ownership: &Graph, members: &Graph) -> Result<(Paths, Value)> {
    let mut graph = ownership.clone();
    graph.extend(members.iter());
    let types = subjects(&graph, &rdf_type(), &n("Type"))?;
    let mut declared = subjects(members, &rdf_type(), &n("Operation"))?;
    declared.extend(subjects(members, &rdf_type(), &n("Constant"))?);
    let bound = members
        .triples_for_predicate(&n("member"))
        .map(|r| named(r.object))
        .collect::<Result<BTreeSet<_>>>()?;
    require(bound == declared, "unbound or undeclared member")?;
    for item in &declared {
        require(
            types.contains(&object_iri(&graph, item, &n("owner"))?),
            "member has an unknown owner",
        )?;
    }
    let pkg = load(&package())?;
    let mut paths = BTreeMap::new();
    let mut expected = json!({});
    for owner in types {
        let binding = one(subjects(&graph, &n("type"), &owner)?.into_iter())?;
        let path = |prop: &str| -> Result<Value> {
            let spelling = literal(&graph, &binding, &n(prop))?;
            Ok(json!(spelling.split("::").collect::<Vec<_>>()))
        };
        paths.insert(
            owner.as_str().into(),
            [path("originalPath")?, path("generatedPath")?],
        );
        let component = object_iri(&graph, &owner, &n("semanticComponent"))?;
        let kind = literal(&graph, &component, &n("componentKind"))?;
        let input = one(arr(&pkg["components"])?
            .iter()
            .filter(|c| c["kind"] == kind))?;
        let overlay = load(&portable().join(txt(&input["overlay"]["path"])?))?;
        let mut fields = Vec::new();
        for item in subjects(&graph, &n("owner"), &owner)? {
            let member_kind = object_iri(&graph, &item, &rdf_type())?;
            let binding = one(subjects(&graph, &n("member"), &item)?.into_iter())?;
            let spelling = literal(&graph, &binding, &n("spelling"))?;
            let (field_kind, actual) = if member_kind == n("Constant") {
                let pred = object_iri(&graph, &item, &n("parameterPredicate"))?;
                let key = pred
                    .as_str()
                    .strip_prefix(CORE)
                    .ok_or("unexpected constant predicate namespace")?;
                let value = if kind == "value-domain" {
                    require(key == "maximum", "wrong range predicate")?;
                    &overlay["maximumConstant"]
                } else {
                    &overlay["constants"][key]
                };
                ("assoc_const", value)
            } else {
                require(member_kind == n("Operation"), "unknown member kind")?;
                let role = object_iri(&graph, &item, &n("role"))?;
                let key = match role.as_str().strip_prefix(NS) {
                    Some("readValue") if kind == "value-domain" => "accessor",
                    Some("readValue") => "getter",
                    Some("checkedConstruction") => "constructor",
                    Some("encode") => "encode",
                    Some("decode") => "decode",
                    _ => return Err("unknown operation role".into()),
                };
                ("function", &overlay[key])
            };
            require(
                actual == spelling.as_str(),
                "member role differs from the pinned target binding",
            )?;
            fields.push((field_kind, spelling));
        }
        fields.sort();
        expected[owner.as_str()] = json!(
            fields
                .into_iter()
                .map(|(kind, name)| json!({"kind":kind,"name":name}))
                .collect::<Vec<_>>()
        );
    }
    Ok((paths, expected))
}
pub fn checked(directory: &Path) -> Result<(Paths, Value)> {
    let file = directory.join("ownership.ttl");
    require(
        sha(&file)? == "87b74e908d4f953d517b9c0ea6e1b6065a1f1e5ca60a55ec961d47cb9d1dd2d6",
        "unexpected portable type ownership graph",
    )?;
    let base = parse(&file)?;
    let mut members = parse(&directory.join("members.ttl"))?;
    let expected = inspect(&base, &members)?;
    let encoder = one(subjects(&members, &n("role"), &n("encode"))?.into_iter())?;
    let role = n("role");
    let old = Triple::new(encoder.clone(), role.clone(), n("encode"));
    members.remove(&old);
    members.insert(&Triple::new(encoder, role, n("decode")));
    let error = inspect(&base, &members)
        .err()
        .ok_or("accepted swapped encode/decode roles")?;
    require(
        error.to_string() == "member role differs from the pinned target binding",
        error.to_string(),
    )?;
    Ok(expected)
}
