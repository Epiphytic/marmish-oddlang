use core_qualification::{ownership::*, *};
use oxigraph::io::RdfFormat;
use oxrdf::{Graph, Term, Triple};
use std::collections::HashSet;
use std::path::Path;

type Selection = (Term, Option<Term>);
fn expectations(path: &Path, kind: &str, role: &str) -> Result<HashSet<Selection>> {
    if kind == "endpoint-set" {
        let id = load(path)?;
        return Ok([(oxrdf::NamedNode::new(txt(&id["id"])?)?.into(), None)].into());
    }
    let graph = parse(path, RdfFormat::Turtle)?;
    let class = match kind {
        "value-domain" => "OptionalUnsignedRange",
        "json-envelope" => "JsonEnvelope",
        _ => return Err("unexpected component kind".into()),
    };
    let root = one(members(&graph, &c(class))?.into_iter())?;
    let mut limits = HashSet::new();
    if kind == "json-envelope" {
        for name in ["rawBytes", "wireBytes", "recordLineBytes"] {
            let p = c(name);
            require(
                matches!(single(&graph,&root,&p)?,Term::Literal(l) if l.datatype()==oxrdf::vocab::xsd::INTEGER),
                "invalid envelope limit",
            )?;
            limits.insert((p.into(), Some(root.clone().into())));
        }
    }
    match role {
        "domain" => Ok([(root.into(), None)].into()),
        "errorCase" => Ok(members(&graph, &c("NamedError"))?
            .into_iter()
            .map(|s| (s.into(), None))
            .collect()),
        "parameterPredicate" => Ok(limits),
        _ => Err("unexpected semantic role".into()),
    }
}
fn resolve(g: &Graph) -> Result<Vec<Row>> {
    require(
        sha(&package())? == "89292d721acd668379be55e762b963bfac16f0c57fad916c450ca14f0658e1d4",
        "unexpected core manifest",
    )?;
    let manifest = load(&package())?;
    let rows = query(g, SEMANTICS)?;
    let types: HashSet<_> = TYPES.iter().map(|t| m(t.1)).collect();
    require(
        members(g, &o("Type"))? == types,
        "unexpected type inventory",
    )?;
    require(
        rows.iter()
            .map(|r| named(&r["type"]).cloned())
            .collect::<Result<HashSet<_>>>()?
            == types,
        "missing semantic roles",
    )?;
    for (_, slug, kind, role) in TYPES {
        let ty = m(slug);
        let component = single(g, &ty, &o("semanticComponent"))?;
        let component = named(&component)?;
        require(
            g.contains(&Triple::new(component.clone(), rdf_type(), o("Component"))),
            "undeclared semantic component",
        )?;
        let spec = &one(arr(&manifest["components"])?
            .iter()
            .filter(|s| s["kind"] == kind))?["graph"];
        let relative = txt(&spec["path"])?;
        let path = portable().join(relative);
        let digest = sha(&path)?;
        require(spec["sha256"] == digest, "component graph digest mismatch")?;
        require(
            single(g, component, &o("componentKind"))? == literal(kind)
                && single(g, component, &o("inMember"))? == m("canonical-member").into(),
            "wrong semantic component",
        )?;
        for key in [
            "domain",
            "errorCase",
            "parameterSubject",
            "parameterPredicate",
        ] {
            require(
                key == role
                    || (key == "parameterSubject" && role == "parameterPredicate")
                    || g.objects_for_subject_predicate(&ty, &o(key))
                        .next()
                        .is_none(),
                "unexpected semantic role",
            )?;
        }
        let selected: Vec<_> = rows
            .iter()
            .filter(|r| r["type"] == ty.clone().into())
            .collect();
        let source = iri(&format!("{PIN}{relative}")).into();
        require(
            selected.iter().all(|r| {
                r["role"] == o(role).into()
                    && r["digest"] == literal(&digest)
                    && r["source"] == source
            }),
            "wrong component pin or semantic role",
        )?;
        let expected = expectations(&path, kind, role)?;
        require(
            selected.len() == expected.len()
                && selected
                    .iter()
                    .map(|r| (r["term"].clone(), r.get("subject").cloned()))
                    .collect::<HashSet<_>>()
                    == expected,
            format!("wrong semantic selections for {slug}"),
        )?;
    }
    Ok(rows)
}
fn main() -> Result {
    let mut graph = ownership()?;
    let complete = resolve(&graph)?;
    remove_native(&mut graph)?;
    require(
        resolve(&graph)? == complete,
        "semantic resolution depends on native bindings",
    )?;
    println!(
        "{} types resolve to {} domain, case and parameter selections without native bindings",
        TYPES.len(),
        complete.len()
    );
    let wrong = m("configuration-failure");
    set(
        &mut graph,
        &wrong,
        &o("semanticComponent"),
        m("probe-envelope-domain"),
    );
    rejected(resolve(&graph), "wrong semantic component")?;
    println!("rejected substituted aggregate error component");
    set(
        &mut graph,
        &wrong,
        &o("semanticComponent"),
        m("retention-domain"),
    );
    set(
        &mut graph,
        &m("retention-domain"),
        &o("graphSha256"),
        literal(&"0".repeat(64)),
    );
    rejected(resolve(&graph), "wrong component pin or semantic role")?;
    println!("rejected substituted graph digest");
    Ok(())
}
