//! Prompt manager for loading and rendering Jinja templates.
//!
//! This module provides the [`PromptManager`] struct for managing prompt templates
//! using the `MiniJinja` template engine.
//!
//! # Example
//!
//! ```no_run
//! use gba_pm::PromptManager;
//! use serde_json::json;
//!
//! let mut manager = PromptManager::new();
//!
//! // Add a template from a string
//! manager.add("greeting", "Hello, {{ name }}!").unwrap();
//!
//! // Render the template
//! let result = manager.render("greeting", json!({"name": "World"})).unwrap();
//! assert_eq!(result, "Hello, World!");
//! ```

use std::fs;
use std::path::Path;

use minijinja::Environment;
use serde::Serialize;
use tracing::{debug, instrument};

use crate::error::{PromptError, Result};

/// Template file extensions that will be loaded from directories.
const TEMPLATE_EXTENSIONS: &[&str] = &["j2", "jinja", "jinja2"];

/// Prompt manager for loading and rendering Jinja templates.
///
/// The manager uses `MiniJinja` as the template engine and supports loading
/// templates from directories or adding them programmatically.
#[derive(Debug)]
#[non_exhaustive]
pub struct PromptManager<'a> {
    /// The `MiniJinja` environment containing all templates.
    env: Environment<'a>,
}

impl Default for PromptManager<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptManager<'_> {
    /// Create a new prompt manager with an empty template environment.
    ///
    /// # Example
    ///
    /// ```
    /// use gba_pm::PromptManager;
    ///
    /// let manager = PromptManager::new();
    /// assert!(manager.names().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
        }
    }

    /// Load templates from a directory.
    ///
    /// This method recursively scans the given directory for template files
    /// with extensions `.j2`, `.jinja`, or `.jinja2`. Template names are derived
    /// from the relative path with the extension stripped.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory containing templates
    ///
    /// # Returns
    ///
    /// Returns `&mut Self` to allow method chaining.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The directory cannot be read
    /// - A template file cannot be read
    /// - A template has invalid syntax
    ///
    /// # Example
    ///
    /// ```no_run
    /// use gba_pm::PromptManager;
    ///
    /// let mut manager = PromptManager::new();
    /// manager.load_dir("./prompts")?;
    /// # Ok::<(), gba_pm::PromptError>(())
    /// ```
    #[instrument(skip(self), fields(path = %path.as_ref().display()))]
    pub fn load_dir(&mut self, path: impl AsRef<Path>) -> Result<&mut Self> {
        let path = path.as_ref();
        self.load_dir_recursive(path, path)?;
        Ok(self)
    }

    /// Recursively load templates from a directory.
    fn load_dir_recursive(&mut self, base: &Path, current: &Path) -> Result<()> {
        let entries = fs::read_dir(current).map_err(|e| PromptError::io_error(current, e))?;

        for entry in entries {
            let entry = entry.map_err(|e| PromptError::io_error(current, e))?;
            let path = entry.path();

            if path.is_dir() {
                self.load_dir_recursive(base, &path)?;
            } else if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy();
                if TEMPLATE_EXTENSIONS.contains(&ext_str.as_ref()) {
                    self.load_template_file(base, &path)?;
                }
            }
        }

        Ok(())
    }

    /// Load a single template file.
    fn load_template_file(&mut self, base: &Path, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path).map_err(|e| PromptError::io_error(path, e))?;

        // Compute template name from relative path, stripping extension
        let relative = path
            .strip_prefix(base)
            .map_err(|e| PromptError::io_error(path, std::io::Error::other(e)))?;

        let name = relative.with_extension("").to_string_lossy().to_string();
        // Normalize path separators to forward slashes for cross-platform consistency
        let name = name.replace('\\', "/");

        debug!(template = %name, "loading template");
        self.env.add_template_owned(name, content)?;

        Ok(())
    }

    /// Add a template from a string.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to register the template under
    /// * `content` - The template content
    ///
    /// # Returns
    ///
    /// Returns `&mut Self` to allow method chaining.
    ///
    /// # Errors
    ///
    /// Returns an error if the template has invalid syntax.
    ///
    /// # Example
    ///
    /// ```
    /// use gba_pm::PromptManager;
    ///
    /// let mut manager = PromptManager::new();
    /// manager.add("hello", "Hello, {{ name }}!")?;
    /// # Ok::<(), gba_pm::PromptError>(())
    /// ```
    pub fn add(&mut self, name: &str, content: &str) -> Result<&mut Self> {
        debug!(template = %name, "adding template");
        self.env
            .add_template_owned(name.to_string(), content.to_string())?;
        Ok(self)
    }

    /// Render a template with the given context.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the template to render
    /// * `ctx` - The context data to pass to the template
    ///
    /// # Returns
    ///
    /// Returns the rendered template as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The template is not found
    /// - The template cannot be rendered with the given context
    ///
    /// # Example
    ///
    /// ```
    /// use gba_pm::PromptManager;
    /// use serde_json::json;
    ///
    /// let mut manager = PromptManager::new();
    /// manager.add("greeting", "Hello, {{ name }}!")?;
    ///
    /// let result = manager.render("greeting", json!({"name": "World"}))?;
    /// assert_eq!(result, "Hello, World!");
    /// # Ok::<(), gba_pm::PromptError>(())
    /// ```
    #[instrument(skip(self, ctx), fields(template = %name))]
    pub fn render(&self, name: &str, ctx: impl Serialize) -> Result<String> {
        let template = self
            .env
            .get_template(name)
            .map_err(|_| PromptError::TemplateNotFound(name.to_string()))?;

        let result = template.render(ctx)?;
        Ok(result)
    }

    /// Render a string template directly without registering it.
    ///
    /// This is useful for one-off template rendering where you don't need
    /// to store the template for later use.
    ///
    /// # Arguments
    ///
    /// * `template` - The template string to render
    /// * `ctx` - The context data to pass to the template
    ///
    /// # Returns
    ///
    /// Returns the rendered template as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the template cannot be parsed or rendered.
    ///
    /// # Example
    ///
    /// ```
    /// use gba_pm::PromptManager;
    /// use serde_json::json;
    ///
    /// let manager = PromptManager::new();
    /// let result = manager.render_str("Hello, {{ name }}!", json!({"name": "World"}))?;
    /// assert_eq!(result, "Hello, World!");
    /// # Ok::<(), gba_pm::PromptError>(())
    /// ```
    pub fn render_str(&self, template: &str, ctx: impl Serialize) -> Result<String> {
        let result = self.env.render_str(template, ctx)?;
        Ok(result)
    }

    /// List all registered template names.
    ///
    /// # Returns
    ///
    /// Returns a vector of template names.
    ///
    /// # Example
    ///
    /// ```
    /// use gba_pm::PromptManager;
    ///
    /// let mut manager = PromptManager::new();
    /// manager.add("hello", "Hello!")?;
    /// manager.add("goodbye", "Goodbye!")?;
    ///
    /// let names = manager.names();
    /// assert!(names.contains(&"hello"));
    /// assert!(names.contains(&"goodbye"));
    /// # Ok::<(), gba_pm::PromptError>(())
    /// ```
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.env.templates().map(|(name, _)| name).collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_should_create_empty_manager() {
        let manager = PromptManager::new();
        assert!(manager.names().is_empty());
    }

    #[test]
    fn test_should_add_and_render_template() {
        let mut manager = PromptManager::new();
        manager.add("test", "Hello, {{ name }}!").unwrap();

        let result = manager.render("test", json!({"name": "World"})).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_should_render_string_template() {
        let manager = PromptManager::new();
        let result = manager
            .render_str("Value: {{ value }}", json!({"value": 42}))
            .unwrap();
        assert_eq!(result, "Value: 42");
    }

    #[test]
    fn test_should_return_error_for_missing_template() {
        let manager = PromptManager::new();
        let result = manager.render("nonexistent", json!({}));
        assert!(matches!(result, Err(PromptError::TemplateNotFound(_))));
    }

    #[test]
    fn test_should_list_template_names() {
        let mut manager = PromptManager::new();
        manager.add("alpha", "A").unwrap();
        manager.add("beta", "B").unwrap();
        manager.add("gamma", "C").unwrap();

        let names = manager.names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"gamma"));
    }

    #[test]
    fn test_should_load_templates_from_directory() {
        let temp_dir = TempDir::new().unwrap();
        let templates_path = temp_dir.path();

        // Create template files
        fs::write(templates_path.join("hello.j2"), "Hello, {{ name }}!").unwrap();
        fs::write(templates_path.join("bye.jinja"), "Goodbye, {{ name }}!").unwrap();
        fs::write(templates_path.join("nested.jinja2"), "Nested: {{ value }}").unwrap();

        // Create a subdirectory with templates
        let subdir = templates_path.join("sub");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("inner.j2"), "Inner: {{ data }}").unwrap();

        let mut manager = PromptManager::new();
        manager.load_dir(templates_path).unwrap();

        let names = manager.names();
        assert!(names.contains(&"hello"));
        assert!(names.contains(&"bye"));
        assert!(names.contains(&"nested"));
        assert!(names.contains(&"sub/inner"));

        // Verify rendering works
        let result = manager.render("hello", json!({"name": "World"})).unwrap();
        assert_eq!(result, "Hello, World!");

        let result = manager
            .render("sub/inner", json!({"data": "test"}))
            .unwrap();
        assert_eq!(result, "Inner: test");
    }

    #[test]
    fn test_should_ignore_non_template_files() {
        let temp_dir = TempDir::new().unwrap();
        let templates_path = temp_dir.path();

        fs::write(templates_path.join("valid.j2"), "Valid").unwrap();
        fs::write(templates_path.join("ignored.txt"), "Ignored").unwrap();
        fs::write(templates_path.join("readme.md"), "Readme").unwrap();

        let mut manager = PromptManager::new();
        manager.load_dir(templates_path).unwrap();

        let names = manager.names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"valid"));
    }

    #[test]
    fn test_should_support_method_chaining() {
        let mut manager = PromptManager::new();
        manager
            .add("a", "A: {{ x }}")
            .unwrap()
            .add("b", "B: {{ y }}")
            .unwrap();

        assert_eq!(manager.names().len(), 2);
    }

    #[test]
    fn test_should_handle_complex_templates() {
        let mut manager = PromptManager::new();
        let template = r#"
{% for item in items %}
- {{ item.name }}: {{ item.value }}
{% endfor %}
"#;
        manager.add("list", template).unwrap();

        let result = manager
            .render(
                "list",
                json!({
                    "items": [
                        {"name": "foo", "value": 1},
                        {"name": "bar", "value": 2}
                    ]
                }),
            )
            .unwrap();

        assert!(result.contains("foo: 1"));
        assert!(result.contains("bar: 2"));
    }

    #[test]
    fn test_should_return_error_for_invalid_template_syntax() {
        let mut manager = PromptManager::new();
        let result = manager.add("invalid", "{{ unclosed");
        assert!(matches!(result, Err(PromptError::RenderError(_))));
    }
}
