//! Multi-agent orchestration system with customizable system prompts
//!
//! This module provides a lightweight agent system where agents are primarily
//! specialized through custom system prompts while inheriting tools and permissions
//! from the current workspace context.

use crate::error::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

/// Configuration for a single agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// The system prompt that defines the agent's behavior
    /// Required if prompt_file is not provided
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Optional: Load prompt from file instead of inline
    /// Required if prompt is not provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_file: Option<String>,

    /// Optional: Override tools (usually inherits from context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,

    /// Optional: Override permissions (usually inherits from context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

impl AgentConfig {
    /// Validate that the config has either prompt or prompt_file
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.prompt.is_none() && self.prompt_file.is_none() {
            return Err(anyhow::anyhow!(
                "Agent configuration must have either 'prompt' or 'prompt_file'"
            ));
        }
        if self.prompt.is_some() && self.prompt_file.is_some() {
            return Err(anyhow::anyhow!(
                "Agent configuration should have either 'prompt' or 'prompt_file', not both"
            ));
        }
        Ok(())
    }

    /// Get the effective prompt, loading from file if necessary
    pub fn get_prompt(&mut self, agents_dir: Option<&Path>) -> anyhow::Result<String> {
        if let Some(prompt) = &self.prompt {
            return Ok(prompt.clone());
        }

        if let Some(prompt_file) = &self.prompt_file {
            let full_path = if let Some(dir) = agents_dir {
                dir.join(prompt_file)
            } else {
                PathBuf::from(prompt_file)
            };

            let prompt_content = std::fs::read_to_string(&full_path).map_err(|e| {
                anyhow::anyhow!("Cannot read prompt file '{}': {}", full_path.display(), e)
            })?;

            // Cache the loaded prompt
            self.prompt = Some(prompt_content.clone());
            Ok(prompt_content)
        } else {
            Err(anyhow::anyhow!("No prompt or prompt_file specified"))
        }
    }
}

/// Registry of available agents and their configurations
pub struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    #[allow(dead_code)]
    agents_dir: Option<PathBuf>,
}

impl AgentRegistry {
    /// Validate that a prompt file path doesn't escape allowed directories
    fn validate_prompt_path(base_dir: &Path, prompt_file: &str) -> anyhow::Result<PathBuf> {
        let path = if prompt_file.starts_with('/') {
            PathBuf::from(prompt_file)
        } else {
            base_dir.join(prompt_file)
        };

        // Canonicalize to resolve ../ and symlinks
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "Security error: Prompt file must be within project .codex or ~/.codex directory"
                ));
            }
        };

        // Get the home/.codex directory (personal root)
        let home_codex = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
            .join(".codex");

        // Security check: path must be within the provided base directory (or its children)
        // or within the personal ~/.codex directory
        let base_canonical = base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf());
        if !canonical.starts_with(&base_canonical) && !canonical.starts_with(&home_codex) {
            return Err(anyhow::anyhow!(
                "Security error: Prompt file must be within project .codex or ~/.codex directory"
            ));
        }

        Ok(canonical)
    }

    /// Create a new agent registry, loading project-level then user-level configurations if available
    pub fn new() -> Result<Self> {
        let mut agents = HashMap::new();

        // Add the single default "general" agent
        agents.insert(
            "general".to_string(),
            AgentConfig {
                prompt: Some("You are a helpful AI assistant. Complete the given task efficiently and accurately.".to_string()),
                prompt_file: None,
                tools: None,
                permissions: None,
            }
        );

        // Load project-level then user-level agents, with project taking precedence
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_root = cwd.join(".codex");
        let home_root = Self::get_agents_directory();

        fn load_agents_from(root: &Path) -> HashMap<String, AgentConfig> {
            let mut out = HashMap::new();
            let path = root.join("agents.toml");
            if !path.exists() {
                return out;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                return out;
            };
            let Ok(mut parsed) = toml::from_str::<HashMap<String, AgentConfig>>(&content) else {
                return out;
            };
            for (name, mut config) in parsed.clone() {
                if let Err(e) = config.validate() {
                    tracing::warn!("Invalid agent config for '{}': {}", name, e);
                    parsed.remove(&name);
                    continue;
                }
                if let Some(prompt_file) = &config.prompt_file {
                    if let Ok(safe_path) = AgentRegistry::validate_prompt_path(root, prompt_file) {
                        if let Ok(prompt) = std::fs::read_to_string(&safe_path) {
                            config.prompt = Some(prompt);
                            parsed.insert(name.clone(), config);
                        }
                    }
                }
            }
            for (k, v) in parsed {
                out.insert(k, v);
            }
            out
        }

        let mut merged: HashMap<String, AgentConfig> = HashMap::new();
        for (k, v) in load_agents_from(&project_root) {
            merged.insert(k, v);
        }
        if let Some(ref home) = home_root {
            for (k, v) in load_agents_from(home) {
                merged.entry(k).or_insert(v);
            }
        }
        let agents_dir = if project_root.exists() {
            Some(project_root)
        } else {
            home_root
        };
        for (name, cfg) in merged {
            agents.insert(name, cfg);
        }

        Ok(Self { agents, agents_dir })
    }

    /// Get the agents directory path (~/.codex)
    fn get_agents_directory() -> Option<PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(|home| PathBuf::from(home).join(".codex"))
    }

    /// Get an agent configuration by name
    #[allow(dead_code)]
    pub fn get_agent(&self, name: &str) -> Option<&AgentConfig> {
        self.agents.get(name)
    }

    /// Get the system prompt for an agent (falls back to "general" if not found)
    pub fn get_system_prompt(&self, agent_name: &str) -> String {
        self.agents
            .get(agent_name)
            .or_else(|| self.agents.get("general"))
            .and_then(|config| config.prompt.clone())
            .unwrap_or_else(|| "You are a helpful AI assistant.".to_string())
    }

    /// List all available agents
    #[allow(dead_code)]
    pub fn list_agents(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }

    /// Get detailed information about all agents
    pub fn list_agent_details(&self) -> Vec<crate::protocol::AgentInfo> {
        let mut agents = Vec::new();

        for (name, config) in &self.agents {
            let description = if let Some(ref prompt) = config.prompt {
                self.extract_description(prompt)
            } else {
                "Agent with file-based prompt".to_string()
            };
            agents.push(crate::protocol::AgentInfo {
                name: name.clone(),
                description,
                is_builtin: name == "general",
            });
        }

        agents.sort_by(|a, b| {
            // Built-in agents first, then alphabetical
            match (a.is_builtin, b.is_builtin) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        agents
    }

    /// Extract brief description from prompt
    fn extract_description(&self, prompt: &str) -> String {
        // Take first line or first sentence as description
        let first_line = prompt.lines().next().unwrap_or("");
        let desc = if let Some(pos) = first_line.find('.') {
            &first_line[..=pos]
        } else {
            first_line
        };

        // Clean up common prefixes
        desc.trim_start_matches("You are a ")
            .trim_start_matches("You are an ")
            .trim_start_matches("You are ")
            .trim()
            .to_string()
    }

    /// Check if agents can spawn other agents (always false to prevent recursion)
    #[allow(dead_code)]
    pub fn can_spawn_agents(metadata: &HashMap<String, String>) -> bool {
        !metadata.contains_key("is_agent")
    }

    /// Mark a context as being an agent context
    #[allow(dead_code)]
    pub fn mark_as_agent_context(metadata: &mut HashMap<String, String>) {
        metadata.insert("is_agent".to_string(), "true".to_string());
    }
}

/// Execute an agent with a specific task
#[allow(dead_code)]
pub async fn execute_agent_task(
    agent_name: &str,
    task: String,
    registry: &AgentRegistry,
) -> Result<String> {
    // Get the agent's system prompt
    let system_prompt = registry.get_system_prompt(agent_name);

    // Build the specialized prompt for this agent
    let full_prompt = format!("{system_prompt}\n\nTask: {task}");

    // Note: The actual execution will be handled by the parent context
    // using the existing conversation infrastructure
    Ok(full_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_agent_exists() {
        let registry = AgentRegistry::new().unwrap();
        assert!(registry.get_agent("general").is_some());
    }

    #[test]
    fn test_agent_recursion_prevention() {
        let mut metadata = HashMap::new();
        assert!(AgentRegistry::can_spawn_agents(&metadata));

        AgentRegistry::mark_as_agent_context(&mut metadata);
        assert!(!AgentRegistry::can_spawn_agents(&metadata));
    }

    #[test]
    fn test_path_traversal_prevention() {
        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path();

        // Create a safe file
        let safe_dir = base_dir.join("prompts");
        fs::create_dir(&safe_dir).unwrap();
        let safe_file = safe_dir.join("test.txt");
        fs::write(&safe_file, "safe content").unwrap();

        // Test that normal paths work
        let result = AgentRegistry::validate_prompt_path(base_dir, "prompts/test.txt");
        assert!(result.is_ok());

        // Test that path traversal is blocked
        let result = AgentRegistry::validate_prompt_path(base_dir, "../../../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Security error"));

        // Test that absolute paths outside allowed dirs are blocked
        let result = AgentRegistry::validate_prompt_path(base_dir, "/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_config_validation() {
        // Test config with prompt is valid
        let config = AgentConfig {
            prompt: Some("Test prompt".to_string()),
            prompt_file: None,
            tools: None,
            permissions: None,
        };
        assert!(config.validate().is_ok());

        // Test config with prompt_file is valid
        let config = AgentConfig {
            prompt: None,
            prompt_file: Some("test.txt".to_string()),
            tools: None,
            permissions: None,
        };
        assert!(config.validate().is_ok());

        // Test config with neither prompt nor prompt_file is invalid
        let config = AgentConfig {
            prompt: None,
            prompt_file: None,
            tools: None,
            permissions: None,
        };
        assert!(config.validate().is_err());

        // Test config with both prompt and prompt_file is invalid
        let config = AgentConfig {
            prompt: Some("Test prompt".to_string()),
            prompt_file: Some("test.txt".to_string()),
            tools: None,
            permissions: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_agent_config_get_prompt() {
        // Test getting prompt from inline prompt
        let mut config = AgentConfig {
            prompt: Some("Inline prompt".to_string()),
            prompt_file: None,
            tools: None,
            permissions: None,
        };
        assert_eq!(config.get_prompt(None).unwrap(), "Inline prompt");

        // Test getting prompt from file
        let temp_dir = TempDir::new().unwrap();
        let prompt_file = temp_dir.path().join("test_prompt.txt");
        fs::write(&prompt_file, "File-based prompt").unwrap();

        let mut config = AgentConfig {
            prompt: None,
            prompt_file: Some("test_prompt.txt".to_string()),
            tools: None,
            permissions: None,
        };

        let prompt = config.get_prompt(Some(temp_dir.path())).unwrap();
        assert_eq!(prompt, "File-based prompt");

        // Check that prompt is cached
        assert_eq!(config.prompt, Some("File-based prompt".to_string()));
    }
}
