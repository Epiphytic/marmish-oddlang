//! Portable ownership graph and its two published SPARQL queries.
use crate::{Result, one, portable, require, whole};
use oxigraph::{
    io::{RdfFormat, RdfParser},
    sparql::{QueryResults, SparqlEvaluator},
    store::Store,
};
use oxrdf::{Graph, GraphName, Literal, NamedNode, Quad, Term, Triple};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

pub const BASE: &str = "036349fc69ab5b850df2006c16ec2518be95a254";
pub const PIN: &str = "https://github.com/Epiphytic/marmish-oddlang/blob/036349fc69ab5b850df2006c16ec2518be95a254/oddlang/portable/";
pub const TYPES: [(&str, &str, &str, &str); 6] = [
    (
        "ConfigError",
        "configuration-failure",
        "value-domain",
        "errorCase",
    ),
    (
        "FrameError",
        "framing-failure",
        "json-envelope",
        "errorCase",
    ),
    (
        "Limits",
        "frame-limits",
        "json-envelope",
        "parameterPredicate",
    ),
    ("ProbeEnvelope", "probe-envelope", "json-envelope", "domain"),
    ("RelaySet", "relay-set", "endpoint-set", "domain"),
    ("Retention", "retention-policy", "value-domain", "domain"),
];
pub fn iri(value: &str) -> NamedNode {
    NamedNode::new_unchecked(value)
}
pub fn m(name: &str) -> NamedNode {
    iri(&format!("urn:marmish:structure:core:{name}"))
}
pub fn o(name: &str) -> NamedNode {
    iri(&format!("urn:oddlang:structural-ownership:{name}"))
}
pub fn c(name: &str) -> NamedNode {
    iri(&format!("urn:oddlang:core:{name}"))
}
pub fn f(name: &str) -> NamedNode {
    iri(&format!("urn:oddlang:facade:{name}"))
}
pub fn binding(name: &str) -> NamedNode {
    iri(&format!("urn:marmish:binding:rust:core:{name}"))
}
pub fn literal(s: &str) -> Term {
    Literal::new_simple_literal(s).into()
}
pub fn rdf_type() -> NamedNode {
    oxrdf::vocab::rdf::TYPE.into_owned()
}
pub fn parse(path: &Path, format: RdfFormat) -> Result<Graph> {
    let mut graph = Graph::new();
    for quad in RdfParser::from_format(format).for_reader(fs::File::open(path)?) {
        let q = quad?;
        graph.insert(&Triple::new(q.subject, q.predicate, q.object));
    }
    Ok(graph)
}
pub fn ownership() -> Result<Graph> {
    parse(
        &portable().join("core-structure-v1/ownership.ttl"),
        RdfFormat::Turtle,
    )
}
pub fn named(term: &Term) -> Result<&NamedNode> {
    match term {
        Term::NamedNode(n) => Ok(n),
        _ => Err("expected an IRI".into()),
    }
}
pub fn value(term: &Term) -> Result<&str> {
    match term {
        Term::Literal(l) => Ok(l.value()),
        Term::NamedNode(n) => Ok(n.as_str()),
        _ => Err("expected an IRI or literal".into()),
    }
}
pub fn single(g: &Graph, s: &NamedNode, p: &NamedNode) -> Result<Term> {
    Ok(one(g.objects_for_subject_predicate(s, p))?.into_owned())
}
pub fn members(g: &Graph, class: &NamedNode) -> Result<HashSet<NamedNode>> {
    g.subjects_for_predicate_object(oxrdf::vocab::rdf::TYPE, class)
        .map(|s| match s {
            oxrdf::NamedOrBlankNodeRef::NamedNode(n) => Ok(n.into_owned()),
            _ => Err("expected named subject".into()),
        })
        .collect()
}
pub fn set(g: &mut Graph, s: &NamedNode, p: &NamedNode, v: impl Into<Term>) {
    let old: Vec<_> = g
        .objects_for_subject_predicate(s, p)
        .map(|v| v.into_owned())
        .collect();
    for obj in old {
        g.remove(&Triple::new(s.clone(), p.clone(), obj));
    }
    g.insert(&Triple::new(s.clone(), p.clone(), v));
}
pub fn remove_native(g: &mut Graph) -> Result {
    let bindings = members(g, &o("RustBinding"))?;
    let hints = ["originalFacade", "rustOverlay", "rustOverlaySha256"].map(o);
    let remove: Vec<_> = g
        .iter()
        .filter(|triple| {
            let native = match triple.subject {
                oxrdf::NamedOrBlankNodeRef::NamedNode(node) => {
                    bindings.contains(&node.into_owned())
                }
                _ => false,
            };
            native || hints.iter().any(|hint| hint.as_ref() == triple.predicate)
        })
        .map(|triple| triple.into_owned())
        .collect();
    for triple in remove {
        g.remove(&triple);
    }
    Ok(())
}
pub type Row = BTreeMap<String, Term>;
pub fn query(g: &Graph, file: &str) -> Result<Vec<Row>> {
    let store = Store::new()?;
    store.extend(g.iter().map(|t| {
        let t = t.into_owned();
        Quad::new(t.subject, t.predicate, t.object, GraphName::DefaultGraph)
    }))?;
    let QueryResults::Solutions(rows) = SparqlEvaluator::new()
        .parse_query(&fs::read_to_string(
            whole().join("core-ownership").join(file),
        )?)?
        .on_store(&store)
        .execute()?
    else {
        return Err("expected SELECT query".into());
    };
    rows.map(|row| {
        Ok(row?
            .iter()
            .map(|(v, t)| (v.as_str().to_owned(), t.clone()))
            .collect())
    })
    .collect()
}
pub fn rejected<T>(result: Result<T>, message: &str) -> Result {
    let err = result.err().ok_or("mutation was accepted")?;
    require(
        err.to_string() == message,
        format!("unexpected mutation outcome: {err}"),
    )
}
pub const SEMANTICS: &str = "semantic-roles.sparql.txt";
