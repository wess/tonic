use crate::ast::{E, K};
use std::collections::{HashMap, HashSet};

fn resolve(name: &str, aliases: &HashMap<String, String>) -> String {
    let name = name.strip_prefix("Elixir.").unwrap_or(name);
    let (head, tail) = name.split_once('.').unwrap_or((name, ""));
    match aliases.get(head) {
        Some(full) if tail.is_empty() => full.clone(),
        Some(full) => format!("{}.{}", full, tail),
        None => name.to_string(),
    }
}

fn scan(
    e: &E,
    parent: &str,
    aliases: &mut HashMap<String, String>,
    declarations: &mut HashSet<String>,
    references: &mut HashSet<String>,
) {
    match &e.k {
        K::Block(statements) => {
            for statement in statements {
                scan(statement, parent, aliases, declarations, references);
            }
            return;
        }
        K::Call { name, args, .. } if name == "defmodule" || name == "defprotocol" => {
            if let Some(E {
                k: K::Alias(name), ..
            }) = args.first()
            {
                let module = if !parent.is_empty() && !name.contains('.') {
                    format!("{}.{}", parent, name)
                } else {
                    resolve(name, aliases)
                };
                declarations.insert(module.clone());
                let mut scope = aliases.clone();
                if let Some(body) = args.last().and_then(|a| a.kw_get("do")) {
                    scan(body, &module, &mut scope, declarations, references);
                }
                return;
            }
        }
        K::Call { name, args, .. } if name == "defimpl" => {
            if let Some(E {
                k: K::Alias(protocol),
                ..
            }) = args.first()
            {
                references.insert(resolve(protocol, aliases));
            }
            if let Some(target) = args.iter().find_map(|argument| argument.kw_get("for")) {
                let targets = match &target.k {
                    K::List(targets, _) => targets.clone(),
                    _ => vec![target.clone()],
                };
                for target in targets {
                    if let K::Alias(name) = &target.k {
                        references.insert(resolve(name, aliases));
                    }
                }
            }
            if let Some(body) = args.iter().find_map(|argument| argument.kw_get("do")) {
                scan(body, parent, &mut aliases.clone(), declarations, references);
            }
            return;
        }
        K::Call { name, args, .. }
            if matches!(name.as_str(), "alias" | "import" | "require" | "use") =>
        {
            if let Some(E {
                k: K::Alias(target),
                ..
            }) = args.first()
            {
                let full = resolve(target, aliases);
                if name != "alias" {
                    references.insert(full.clone());
                }
                let short = args
                    .get(1)
                    .and_then(|a| a.kw_get("as"))
                    .and_then(|a| match &a.k {
                        K::Alias(n) => Some(n.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| target.rsplit('.').next().unwrap().to_string());
                aliases.insert(short, full);
            }
            return;
        }
        _ => {}
    }
    crate::collect::map_expr(e, &mut |node| {
        if let K::Struct { name, .. } = &node.k {
            if let K::Alias(name) = &name.k {
                references.insert(resolve(name, aliases));
            }
        }
        if let K::Call { name, .. } = &node.k {
            if matches!(
                name.as_str(),
                "defmodule" | "defprotocol" | "alias" | "import" | "require" | "use"
            ) {
                scan(node, parent, aliases, declarations, references);
                return Some(node.clone());
            }
        }
        None
    });
}

fn module_only(e: &E) -> bool {
    match &e.k {
        K::Block(statements) => statements.iter().all(module_only),
        K::Call { name, .. }
            if matches!(
                name.as_str(),
                "defmodule" | "defprotocol" | "defimpl" | "alias" | "import" | "require"
            ) =>
        {
            true
        }
        K::Call { name, args, .. }
            if matches!(name.as_str(), "if" | "unless") && args.len() == 2 =>
        {
            args[1].kw_get("do").map(module_only).unwrap_or(false)
                && args[1].kw_get("else").map(module_only).unwrap_or(true)
        }
        _ => false,
    }
}

/// Orders module files by their compile-time dependencies, preserving script order.
pub fn sources(files: &[String]) -> Result<Vec<String>, String> {
    let mut declarations = HashMap::new();
    let mut references = Vec::new();
    let mut reorder = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let mut declared = HashSet::new();
        let mut used = HashSet::new();
        let mut pure = file.ends_with(".erl");
        if file.ends_with(".ex") {
            let source = std::fs::read_to_string(file).map_err(|e| format!("{}: {}", file, e))?;
            let ast = crate::parser::parse_source(&source, file)?;
            pure = ast.iter().all(module_only);
            let mut aliases = HashMap::new();
            for expression in ast {
                scan(&expression, "", &mut aliases, &mut declared, &mut used);
            }
        }
        for module in declared {
            declarations.entry(module).or_insert(index);
        }
        references.push(used);
        reorder.push(pure);
    }
    let dependencies: Vec<HashSet<usize>> = references
        .into_iter()
        .enumerate()
        .map(|(index, used)| {
            used.into_iter()
                .filter_map(|module| declarations.get(&module).copied())
                .filter(|dependency| *dependency != index)
                .collect()
        })
        .collect();
    let mut remaining: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(index, _)| reorder[*index])
        .map(|(i, _)| i)
        .collect();
    let mut emitted = HashSet::new();
    let mut result = Vec::new();
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .position(|index| dependencies[*index].iter().all(|d| emitted.contains(d)))
            .unwrap_or(0);
        let index = remaining.remove(ready);
        emitted.insert(index);
        result.push(files[index].clone());
    }
    let mut ordered = result.into_iter();
    Ok(files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            if !reorder[index] {
                file.clone()
            } else {
                ordered.next().unwrap()
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn protocol_dependencies_reorder_only_module_slots() {
        let root = std::env::temp_dir().join(format!("tonicorder{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let implementation = root.join("implementation.ex");
        let protocol = root.join("protocol.ex");
        let script = root.join("script.exs");
        std::fs::write(
            &implementation,
            "defimpl Conversion, for: Integer do\n def convert(value), do: value\nend\n",
        )
        .unwrap();
        std::fs::write(
            &protocol,
            "defprotocol Conversion do\n def convert(value)\nend\n",
        )
        .unwrap();
        std::fs::write(&script, "IO.puts(:script)\n").unwrap();
        let files = vec![
            script.to_string_lossy().into_owned(),
            implementation.to_string_lossy().into_owned(),
            protocol.to_string_lossy().into_owned(),
        ];
        assert_eq!(
            super::sources(&files).unwrap(),
            vec![files[0].clone(), files[2].clone(), files[1].clone()]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
