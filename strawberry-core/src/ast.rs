//! Pure deterministic multi-language AST and symbolic structural analyzer.
//! Supports TypeScript/JavaScript, Python, Rust, and Go without LLMs or network calls.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    ClassOrStruct,
    InterfaceOrTrait,
    Import,
    ErrorOrThrow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolItem {
    pub kind: SymbolKind,
    pub name: String,
    pub signature: String,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolicAnalysis {
    pub language: String,
    pub total_lines: usize,
    pub imports: Vec<String>,
    pub functions: Vec<SymbolItem>,
    pub types_or_classes: Vec<SymbolItem>,
    pub error_points: Vec<SymbolItem>,
}

/// Analyze source code deterministically based on language name or extension.
pub fn analyze_source(lang_or_ext: &str, source: &str) -> SymbolicAnalysis {
    let lower = lang_or_ext.to_lowercase();
    let lang = match lower.as_str() {
        "ts" | "tsx" | "typescript" => "typescript",
        "js" | "jsx" | "javascript" => "javascript",
        "py" | "python" => "python",
        "rs" | "rust" => "rust",
        "go" | "golang" => "go",
        other => other,
    };

    let mut analysis = SymbolicAnalysis {
        language: lang.to_string(),
        total_lines: source.lines().count(),
        ..Default::default()
    };

    let mut seen_imports = HashSet::new();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        match lang {
            "typescript" | "javascript" => {
                parse_js_ts_line(trimmed, line_num, &mut analysis, &mut seen_imports);
            }
            "python" => {
                parse_python_line(trimmed, line_num, &mut analysis, &mut seen_imports);
            }
            "rust" => {
                parse_rust_line(trimmed, line_num, &mut analysis, &mut seen_imports);
            }
            "go" => {
                parse_go_line(trimmed, line_num, &mut analysis, &mut seen_imports);
            }
            _ => {
                // Generic structural heuristics fallback
                parse_generic_line(trimmed, line_num, &mut analysis, &mut seen_imports);
            }
        }
    }

    analysis
}

fn parse_js_ts_line(
    line: &str,
    line_num: usize,
    analysis: &mut SymbolicAnalysis,
    seen_imports: &mut HashSet<String>,
) {
    if line.starts_with("import ") || line.starts_with("import{") {
        if let Some(pkg) = extract_js_import(line) {
            if seen_imports.insert(pkg.clone()) {
                analysis.imports.push(pkg);
            }
        }
    } else if line.contains("function ") || line.contains("=>") || line.starts_with("export const ") || line.starts_with("const ") {
        if let Some(fn_item) = parse_js_fn(line, line_num) {
            analysis.functions.push(fn_item);
        }
    } else if line.starts_with("class ") || line.starts_with("export class ") || line.starts_with("interface ") || line.starts_with("export interface ") || line.starts_with("type ") || line.starts_with("export type ") {
        if let Some(type_item) = parse_js_type(line, line_num) {
            analysis.types_or_classes.push(type_item);
        }
    }

    if line.contains("throw new ") || line.contains("throw ") || line.contains("console.error(") {
        analysis.error_points.push(SymbolItem {
            kind: SymbolKind::ErrorOrThrow,
            name: "throw/error".to_string(),
            signature: truncate(line, 80),
            line: line_num,
        });
    }
}

fn parse_python_line(
    line: &str,
    line_num: usize,
    analysis: &mut SymbolicAnalysis,
    seen_imports: &mut HashSet<String>,
) {
    if line.starts_with("import ") || line.starts_with("from ") {
        let pkg = line.split_whitespace().nth(1).unwrap_or("").to_string();
        if !pkg.is_empty() && seen_imports.insert(pkg.clone()) {
            analysis.imports.push(pkg);
        }
    } else if line.starts_with("def ") || line.starts_with("async def ") {
        let name_part = line.trim_start_matches("async ").trim_start_matches("def ");
        let name = name_part.split('(').next().unwrap_or("").trim().to_string();
        if !name.is_empty() {
            analysis.functions.push(SymbolItem {
                kind: SymbolKind::Function,
                name,
                signature: truncate(line, 80),
                line: line_num,
            });
        }
    } else if line.starts_with("class ") {
        let name = line.trim_start_matches("class ").split(&['(', ':'][..]).next().unwrap_or("").trim().to_string();
        if !name.is_empty() {
            analysis.types_or_classes.push(SymbolItem {
                kind: SymbolKind::ClassOrStruct,
                name,
                signature: truncate(line, 80),
                line: line_num,
            });
        }
    }

    if line.starts_with("raise ") || line.contains("except ") {
        analysis.error_points.push(SymbolItem {
            kind: SymbolKind::ErrorOrThrow,
            name: "raise/except".to_string(),
            signature: truncate(line, 80),
            line: line_num,
        });
    }
}

fn parse_rust_line(
    line: &str,
    line_num: usize,
    analysis: &mut SymbolicAnalysis,
    seen_imports: &mut HashSet<String>,
) {
    if line.starts_with("use ") || line.starts_with("pub use ") {
        let path = line.trim_start_matches("pub ").trim_start_matches("use ").trim_end_matches(';').to_string();
        if seen_imports.insert(path.clone()) {
            analysis.imports.push(path);
        }
    } else if line.contains("fn ") {
        if let Some(idx) = line.find("fn ") {
            let after = &line[idx + 3..];
            let name = after.split(&['(', '<', ' '][..]).next().unwrap_or("").trim().to_string();
            if !name.is_empty() {
                analysis.functions.push(SymbolItem {
                    kind: SymbolKind::Function,
                    name,
                    signature: truncate(line, 80),
                    line: line_num,
                });
            }
        }
    } else if line.contains("struct ") || line.contains("enum ") || line.contains("trait ") {
        for keyword in &["struct ", "enum ", "trait "] {
            if let Some(idx) = line.find(keyword) {
                let after = &line[idx + keyword.len()..];
                let name = after.split(&['{', '(', '<', ';', ' '][..]).next().unwrap_or("").trim().to_string();
                if !name.is_empty() {
                    let kind = if *keyword == "trait " {
                        SymbolKind::InterfaceOrTrait
                    } else {
                        SymbolKind::ClassOrStruct
                    };
                    analysis.types_or_classes.push(SymbolItem {
                        kind,
                        name,
                        signature: truncate(line, 80),
                        line: line_num,
                    });
                    break;
                }
            }
        }
    }

    if line.contains("panic!(") || line.contains("Err(") || line.contains("bail!(") {
        analysis.error_points.push(SymbolItem {
            kind: SymbolKind::ErrorOrThrow,
            name: "panic/err".to_string(),
            signature: truncate(line, 80),
            line: line_num,
        });
    }
}

fn parse_go_line(
    line: &str,
    line_num: usize,
    analysis: &mut SymbolicAnalysis,
    seen_imports: &mut HashSet<String>,
) {
    if line.starts_with("import ") {
        let pkg = line.trim_start_matches("import ").trim_matches('"').to_string();
        if seen_imports.insert(pkg.clone()) {
            analysis.imports.push(pkg);
        }
    } else if line.starts_with("func ") {
        let after = line.trim_start_matches("func ");
        let name = after.split('(').next().unwrap_or("").trim().to_string();
        if !name.is_empty() {
            analysis.functions.push(SymbolItem {
                kind: SymbolKind::Function,
                name,
                signature: truncate(line, 80),
                line: line_num,
            });
        }
    } else if line.starts_with("type ") {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1].to_string();
            analysis.types_or_classes.push(SymbolItem {
                kind: SymbolKind::ClassOrStruct,
                name,
                signature: truncate(line, 80),
                line: line_num,
            });
        }
    }

    if line.contains("panic(") || line.contains("fmt.Errorf(") || line.contains("errors.New(") {
        analysis.error_points.push(SymbolItem {
            kind: SymbolKind::ErrorOrThrow,
            name: "panic/error".to_string(),
            signature: truncate(line, 80),
            line: line_num,
        });
    }
}

fn parse_generic_line(
    line: &str,
    line_num: usize,
    analysis: &mut SymbolicAnalysis,
    seen_imports: &mut HashSet<String>,
) {
    if line.contains("import") || line.contains("include") || line.contains("require") {
        if seen_imports.insert(line.to_string()) {
            analysis.imports.push(truncate(line, 50));
        }
    } else if line.contains("fn ") || line.contains("def ") || line.contains("function ") {
        analysis.functions.push(SymbolItem {
            kind: SymbolKind::Function,
            name: "fn".to_string(),
            signature: truncate(line, 80),
            line: line_num,
        });
    }
}

fn extract_js_import(line: &str) -> Option<String> {
    if let Some(idx) = line.find("from ") {
        let pkg = line[idx + 5..].trim().trim_matches(&['\'', '"', ';'][..]);
        if !pkg.is_empty() {
            return Some(pkg.to_string());
        }
    }
    None
}

fn parse_js_fn(line: &str, line_num: usize) -> Option<SymbolItem> {
    if line.contains("function ") {
        let idx = line.find("function ")?;
        let after = &line[idx + 9..];
        let name = after.split('(').next()?.trim().to_string();
        if !name.is_empty() {
            return Some(SymbolItem {
                kind: SymbolKind::Function,
                name,
                signature: truncate(line, 80),
                line: line_num,
            });
        }
    } else if line.contains("=") && line.contains("=>") {
        let parts: Vec<&str> = line.split('=').collect();
        if !parts.is_empty() {
            let name_part = parts[0].trim_start_matches("export ").trim_start_matches("const ").trim_start_matches("let ").trim();
            if !name_part.is_empty() && !name_part.contains(' ') {
                return Some(SymbolItem {
                    kind: SymbolKind::Function,
                    name: name_part.to_string(),
                    signature: truncate(line, 80),
                    line: line_num,
                });
            }
        }
    }
    None
}

fn parse_js_type(line: &str, line_num: usize) -> Option<SymbolItem> {
    for kw in &["class ", "interface ", "type "] {
        if let Some(idx) = line.find(kw) {
            let after = &line[idx + kw.len()..];
            let name = after.split(&[' ', '{', '<', '='][..]).next()?.trim().to_string();
            if !name.is_empty() {
                let kind = match *kw {
                    "interface " => SymbolKind::InterfaceOrTrait,
                    _ => SymbolKind::ClassOrStruct,
                };
                return Some(SymbolItem {
                    kind,
                    name,
                    signature: truncate(line, 80),
                    line: line_num,
                });
            }
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_analysis() {
        let code = r#"
import { useState } from 'react';
import { api } from './api';

export interface User {
  id: string;
}

export function fetchUser(id: string): User {
  if (!id) throw new Error("ID required");
  return { id };
}

const formatUser = (user: User) => user.id;
"#;
        let res = analyze_source("ts", code);
        assert_eq!(res.language, "typescript");
        assert!(res.imports.contains(&"react".to_string()));
        assert!(res.imports.contains(&"./api".to_string()));
        assert!(res.types_or_classes.iter().any(|t| t.name == "User"));
        assert!(res.functions.iter().any(|f| f.name == "fetchUser"));
        assert!(res.functions.iter().any(|f| f.name == "formatUser"));
        assert_eq!(res.error_points.len(), 1);
    }

    #[test]
    fn test_rust_analysis() {
        let code = r#"
use std::collections::HashMap;

pub struct AppState {
    pub count: u32,
}

pub fn run_app() -> Result<(), String> {
    if false {
        panic!("fatal");
    }
    Ok(())
}
"#;
        let res = analyze_source("rs", code);
        assert_eq!(res.language, "rust");
        assert!(res.imports.contains(&"std::collections::HashMap".to_string()));
        assert!(res.types_or_classes.iter().any(|t| t.name == "AppState"));
        assert!(res.functions.iter().any(|f| f.name == "run_app"));
        assert_eq!(res.error_points.len(), 1);
    }
}
