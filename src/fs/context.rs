//! File context ingestion and dependency tracking.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "html", "htm", "css", "scss", "sass", "js", "jsx", "ts", "tsx", "json", "md", "mdx",
];

#[derive(Debug, Clone)]
pub struct FileContext {
    pub path: PathBuf,
    pub content: String,
    pub language: String,
    #[allow(dead_code)]
    pub imports: Vec<String>,
    #[allow(dead_code)]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    pub files: Vec<FileContext>,
    pub frameworks: HashSet<String>,
}

impl DependencyGraph {
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} files", self.files.len())];
        if !self.frameworks.is_empty() {
            let fw: Vec<_> = self.frameworks.iter().cloned().collect();
            parts.push(format!("frameworks: {}", fw.join(", ")));
        }
        parts.join(" | ")
    }
}

pub fn detect_language(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("tsx") => "tsx".to_string(),
        Some("ts") => "typescript".to_string(),
        Some("jsx") => "jsx".to_string(),
        Some("js") => "javascript".to_string(),
        Some("css") => "css".to_string(),
        Some("scss") | Some("sass") => "scss".to_string(),
        Some("html") | Some("htm") => "html".to_string(),
        Some("json") => "json".to_string(),
        Some("md") | Some("mdx") => "markdown".to_string(),
        _ => "text".to_string(),
    }
}

pub fn extract_imports(content: &str, language: &str) -> Vec<String> {
    let mut imports = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        match language {
            "tsx" | "jsx" | "typescript" | "javascript" => {
                if trimmed.starts_with("import ") {
                    imports.push(trimmed.to_string());
                } else if trimmed.starts_with("from ") {
                    imports.push(trimmed.to_string());
                }
            }
            "css" | "scss" => {
                if trimmed.starts_with("@import") {
                    imports.push(trimmed.to_string());
                }
            }
            "html" => {
                if trimmed.contains("<link") || trimmed.contains("<script") {
                    imports.push(trimmed.to_string());
                }
            }
            _ => {}
        }
    }

    imports
}

pub fn ingest_file(path: &Path) -> Result<FileContext> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let language = detect_language(path);
    let imports = extract_imports(&content, &language);
    let size_bytes = fs::metadata(path)?.len();

    Ok(FileContext {
        path: path.to_path_buf(),
        content,
        language,
        imports,
        size_bytes,
    })
}

pub fn ingest_path(path: &Path) -> Result<DependencyGraph> {
    let mut graph = DependencyGraph::default();

    if path.is_file() {
        let ctx = ingest_file(path)?;
        detect_frameworks(&ctx, &mut graph.frameworks);
        graph.files.push(ctx);
        return Ok(graph);
    }

    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }

        if should_skip(entry_path) {
            continue;
        }

        if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
            if !SUPPORTED_EXTENSIONS.contains(&ext) {
                continue;
            }
        } else {
            continue;
        }

        if let Ok(ctx) = ingest_file(entry_path) {
            detect_frameworks(&ctx, &mut graph.frameworks);
            graph.files.push(ctx);
        }
    }

    Ok(graph)
}

fn should_skip(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("node_modules")
        || path_str.contains(".git")
        || path_str.contains("target")
        || path_str.contains(".next")
        || path_str.contains("dist")
        || path_str.contains("build")
}

fn detect_frameworks(ctx: &FileContext, frameworks: &mut HashSet<String>) {
    let lower = ctx.content.to_lowercase();
    let name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if name == "tailwind.config.js" || name == "tailwind.config.ts" {
        frameworks.insert("tailwind".to_string());
    }
    if name == "next.config.js" || name == "next.config.ts" || name == "next.config.mjs" {
        frameworks.insert("next.js".to_string());
    }
    if lower.contains("from \"react\"") || lower.contains("from 'react'") {
        frameworks.insert("react".to_string());
    }
    if lower.contains("@tailwind") || lower.contains("tailwindcss") {
        frameworks.insert("tailwind".to_string());
    }
}
