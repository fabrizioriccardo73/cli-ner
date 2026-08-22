use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported software ecosystem / project types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Gradle,
    Composer,
    Go,
    Flutter,
}

impl ProjectType {
    pub fn name(&self) -> &'static str {
        match self {
            ProjectType::Rust => "Rust",
            ProjectType::Node => "Node.js",
            ProjectType::Python => "Python",
            ProjectType::Gradle => "Java/Kotlin (Gradle/Maven)",
            ProjectType::Composer => "PHP (Composer)",
            ProjectType::Go => "Go",
            ProjectType::Flutter => "Flutter/Dart",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ProjectType::Rust => "🦀",
            ProjectType::Node => "🟢",
            ProjectType::Python => "🐍",
            ProjectType::Gradle => "☕",
            ProjectType::Composer => "🐘",
            ProjectType::Go => "🐹",
            ProjectType::Flutter => "💙",
        }
    }

    #[allow(dead_code)]
    pub fn identifier(&self) -> &'static str {
        match self {
            ProjectType::Rust => "rust",
            ProjectType::Node => "node",
            ProjectType::Python => "python",
            ProjectType::Gradle => "gradle",
            ProjectType::Composer => "composer",
            ProjectType::Go => "go",
            ProjectType::Flutter => "flutter",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rust" | "cargo" | "rs" => Some(ProjectType::Rust),
            "node" | "nodejs" | "js" | "ts" | "npm" | "pnpm" | "yarn" => Some(ProjectType::Node),
            "python" | "py" | "pip" => Some(ProjectType::Python),
            "gradle" | "maven" | "java" | "kotlin" | "jvm" => Some(ProjectType::Gradle),
            "composer" | "php" => Some(ProjectType::Composer),
            "go" | "golang" => Some(ProjectType::Go),
            "flutter" | "dart" => Some(ProjectType::Flutter),
            _ => None,
        }
    }
}

/// A specific build or dependency artifact folder inside a project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectArtifact {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub file_count: usize,
    pub description: String,
}

/// A software project discovered during recursive scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredProject {
    pub root_path: PathBuf,
    pub name: String,
    pub project_type: ProjectType,
    pub last_modified: Option<DateTime<Local>>,
    pub days_inactive: Option<u64>,
    pub artifacts: Vec<ProjectArtifact>,
    pub total_reclaimable_bytes: u64,
    pub is_dormant: bool,
}
