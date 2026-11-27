use core_qualification::{ownership::*, *};
use oxigraph::io::RdfFormat;
use oxrdf::{Graph, NamedNode, Triple};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, path::Path, process::Command};

fn pin(revision: &str, path: &Path) -> Result<(NamedNode, String)> {
    let root = whole().ancestors().nth(2).unwrap();
    let relative = path.strip_prefix(root)?.to_str().ok_or("non-UTF-8 path")?;
    let git = Command::new("git")
        .current_dir(root)
        .args(["show", &format!("{revision}:{relative}")])
        .output()?;
    require(
        git.status.success() && git.stdout == fs::read(path)?,
        format!("pinned evidence changed: {relative}"),
    )?;
    Ok((
        iri(&format!(
            "https://github.com/Epiphytic/marmish-oddlang/blob/{revision}/{relative}"
        )),
        format!("{:x}", Sha256::digest(git.stdout)),
    ))
}
fn module(name: &str) -> Result<NamedNode> {
    match name {
        "config" => Ok(m("configuration")),
        "frame" => Ok(m("probe-framing")),
        _ => Err("unknown native module".into()),
    }
}
fn verify(g: &Graph) -> Result<Vec<Row>> {
    let member = m("canonical-member");
    let evidence = "10926d822bb9aa3a16b20ef35cfc63bebd586780";
    let merge = "2c45e8375690c9ff8f25023bf000ced3b16a6762";
    let facade = portable().parent().unwrap().join("facade");
    for (prop, digest, rev, path) in [
        (
            "workspaceLock",
            Some("workspaceLockSha256"),
            BASE,
            workspace_lock(),
        ),
        (
            "packageManifest",
            Some("packageManifestSha256"),
            BASE,
            package(),
        ),
        (
            "standaloneLock",
            Some("standaloneLockSha256"),
            evidence,
            lexical().join("package.lock"),
        ),
        (
            "comparison",
            None,
            evidence,
            lexical().join("comparison.json"),
        ),
        (
            "darwinReceipt",
            None,
            evidence,
            lexical().join(HOSTS[0]).join("receipt.json"),
        ),
        (
            "linuxReceipt",
            None,
            evidence,
            lexical().join(HOSTS[1]).join("receipt.json"),
        ),
        (
            "originalFacadeGraph",
            None,
            merge,
            facade.join("domain.ttl"),
        ),
        (
            "originalFacadeLock",
            None,
            merge,
            facade.join("import.lock"),
        ),
    ] {
        let (uri, hash) = pin(rev, &path)?;
        require(
            single(g, &member, &o(prop))? == uri.into(),
            format!("wrong evidence link for {prop}"),
        )?;
        if let Some(key) = digest {
            require(
                single(g, &member, &o(key))? == literal(&hash),
                format!("wrong digest for {prop}"),
            )?;
        }
    }
    let native = parse(&facade.join("domain.ttl"), RdfFormat::NTriples)?;
    require(
        single(g, &m("package"), &o("originalFacade"))? == f("root").into(),
        "wrong original root mapping",
    )?;
    require(
        g.contains(&Triple::new(
            member.clone(),
            rdf_type(),
            o("PinnedMembership"),
        )) && single(g, &member, &o("member"))? == m("package").into(),
        "wrong pinned member",
    )?;
    require(
        members(g, &o("Module"))? == [module("config")?, module("frame")?].into(),
        "unexpected portable modules",
    )?;
    for name in ["config", "frame"] {
        let owner = module(name)?;
        let original = f(&format!("module:{name}"));
        require(
            single(g, &owner, &o("owner"))? == m("package").into(),
            "wrong module owner",
        )?;
        require(
            single(g, &owner, &o("originalFacade"))? == original.clone().into()
                && native.contains(&Triple::new(f("root"), f("private-module"), original)),
            "wrong original module mapping",
        )?;
    }
    let comparison = comparison::compare()?;
    require(
        format!("{}\n", pretty(&comparison)?)
            == fs::read_to_string(lexical().join("comparison.json"))?,
        "native comparison changed",
    )?;
    let rows = query(g, "ownership.sparql.txt")?;
    let names: HashSet<_> = rows
        .iter()
        .map(|r| value(&r["spelling"]))
        .collect::<Result<_>>()?;
    require(
        rows.len() == TYPES.len() && names == TYPES.iter().map(|t| t.0).collect(),
        "expected one binding per portable data type",
    )?;
    require(
        members(g, &o("Type"))? == TYPES.iter().map(|t| m(t.1)).collect()
            && members(g, &o("RustBinding"))? == TYPES.iter().map(|t| binding(t.1)).collect(),
        "unexpected portable types or target bindings",
    )?;
    for expected in arr(&comparison["publicDataRolePairs"])? {
        let name = txt(&expected["spelling"])?;
        let slug = one(TYPES.iter().filter(|t| t.0 == name))?.1;
        let r = one(rows.iter().filter(|r| r["spelling"] == literal(name)))?;
        let component = &expected["component"];
        let owner = txt(&component["module"])?;
        require(
            r["type"] == m(slug).into() && r["binding"] == binding(slug).into(),
            format!("wrong portable or target identity for {name}"),
        )?;
        require(
            r["owner"] == module(owner)?.into(),
            format!("wrong portable owner for {name}"),
        )?;
        let facade_type = f(&format!("type:{owner}:{name}"));
        require(
            r["facadeType"] == facade_type.clone().into()
                && native.contains(&Triple::new(
                    f(&format!("module:{owner}")),
                    f("public-type"),
                    facade_type,
                )),
            format!("wrong original facade identity for {name}"),
        )?;
        let path = |key: &str| -> Result<String> {
            Ok(arr(&expected[key])?
                .iter()
                .map(txt)
                .collect::<Result<Vec<_>>>()?
                .join("::"))
        };
        for (key, want) in [
            ("kind", txt(&expected["normalizedKind"])?),
            ("role", txt(&expected["nativeRole"])?),
            ("original", &path("originalDeclarationPath")?),
            ("generated", &path("generatedDeclarationPath")?),
            ("componentKind", txt(&component["kind"])?),
        ] {
            require(
                r[key] == literal(want),
                format!("native projection evidence changed for {name}"),
            )?;
        }
        let c = named(&r["component"])?;
        require(
            g.contains(&Triple::new(c.clone(), rdf_type(), o("Component")))
                && single(g, c, &o("inMember"))? == member.clone().into(),
            format!("wrong component membership for {name}"),
        )?;
        for (tag, digest) in [("graph", "graphSha"), ("overlay", "overlaySha")] {
            let p = &component[tag];
            let (uri, hash) = pin(BASE, &portable().join(txt(&p["path"])?))?;
            require(
                r[tag] == uri.into() && value(&r[digest])? == hash && p["sha256"] == hash,
                format!("wrong {tag} pin for {name}"),
            )?;
        }
    }
    Ok(rows)
}
fn main() -> Result {
    let mut graph = ownership()?;
    for row in verify(&graph)? {
        println!(
            "{}\t{}\t{}",
            value(&row["type"])?,
            value(&row["owner"])?,
            value(&row["spelling"])?
        );
    }
    set(
        &mut graph,
        &m("retention-policy"),
        &o("owner"),
        m("probe-framing"),
    );
    let control = "wrong portable owner for Retention";
    rejected(verify(&graph), control)?;
    println!("{control} (rejected)");
    Ok(())
}
