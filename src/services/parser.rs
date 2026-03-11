use anyhow::Result;
use regex::Regex;
use std::collections::BTreeSet;

use crate::models::script::Function;

pub struct PythonParser;

impl PythonParser {
    pub fn parse_functions(script: &str) -> Result<Vec<Function>> {
        let regex = Regex::new(r"(?m)^(?:async\s+)?def\s+(\w+)\s*\(([^)]*)\)(?:\s*->\s*([^:]+))?")?;

        let mut functions = Vec::new();
        for captures in regex.captures_iter(script) {
            let name = captures[1].to_string();
            let params = captures.get(2).map(|value| value.as_str()).unwrap_or("");
            let return_type = captures.get(3).map(|value| value.as_str().trim());
            let signature = return_type
                .map(|value| format!("{name}({params}) -> {value}"))
                .or_else(|| Some(format!("{name}({params})")));

            functions.push(Function {
                name,
                signature,
                description: None,
            });
        }

        Ok(functions)
    }

    pub fn parse_dependencies(script: &str) -> Result<Vec<String>> {
        let import_regex = Regex::new(r"^import\s+(.+)$")?;
        let from_regex = Regex::new(r"^from\s+([A-Za-z_][\w\.]*)\s+import\b")?;
        let mut dependencies = BTreeSet::new();

        for line in script.lines() {
            let trimmed = line.trim_start();

            if let Some(captures) = from_regex.captures(trimmed) {
                dependencies.insert(normalize_module_name(&captures[1]));
                continue;
            }

            if let Some(captures) = import_regex.captures(trimmed) {
                for module in captures[1].split(',') {
                    let normalized = module
                        .split_whitespace()
                        .next()
                        .map(normalize_module_name)
                        .unwrap_or_default();
                    if !normalized.is_empty() {
                        dependencies.insert(normalized);
                    }
                }
            }
        }

        Ok(dependencies.into_iter().collect())
    }
}

fn normalize_module_name(module: &str) -> String {
    module
        .trim()
        .split('.')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::PythonParser;

    #[test]
    fn parses_regular_and_async_functions() {
        let script = "def hello(name: str) -> str:\n    return name\n\nasync def fetch(url):\n    return url\n";
        let functions = PythonParser::parse_functions(script).expect("parse functions");

        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, "hello");
        assert_eq!(
            functions[0].signature.as_ref().expect("signature"),
            "hello(name: str) -> str"
        );
        assert_eq!(functions[1].name, "fetch");
        assert_eq!(
            functions[1].signature.as_ref().expect("signature"),
            "fetch(url)"
        );
    }

    #[test]
    fn parses_and_deduplicates_dependencies() {
        let script = "import json\nfrom pathlib import Path\nimport json\n";
        let dependencies = PythonParser::parse_dependencies(script).expect("parse deps");

        assert_eq!(
            dependencies,
            vec!["json".to_string(), "pathlib".to_string()]
        );
    }

    #[test]
    fn parses_common_import_variants() {
        let script =
            "import os, sys as system\nfrom package.submodule import thing\n    import json\n";
        let dependencies = PythonParser::parse_dependencies(script).expect("parse deps");

        assert_eq!(
            dependencies,
            vec![
                "json".to_string(),
                "os".to_string(),
                "package".to_string(),
                "sys".to_string()
            ]
        );
    }
}
