//! Schema engine (spec §3: schema-driven rendering everywhere).
//!
//! Schemas are declarative JSON files dropped into `/usr/share/webconfig/schemas/`
//! by other Yocto recipes. The frontend contains a generic schema→form renderer
//! (types: string, int, bool, enum, secret; validation rules; grouping; an
//! `apply_hook`). Adding a customer setting must require **zero UI code
//! changes** (spec §3).
//!
//! This module loads schemas, validates config sections against them with the
//! `jsonschema` crate, and exposes the typed [`SchemaField`] shape the frontend
//! renderer consumes (via the API). The same schema drives both frontend
//! rendering and backend validation — the security boundary is the API
//! (spec §2), so every input is re-validated server-side here.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::store::Config;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("schema parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("schema io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("schema validation error: {0}")]
    Validation(String),
}

/// A loaded schema set. The default-permissive variant validates anything
/// (used by tests and first-boot before schemas are installed).
#[derive(Debug, Clone)]
pub struct Schema {
    /// JSON Schema (draft-07) documents keyed by section name, e.g.
    /// "system", "network", or "app:my-app". Wrapped in `Arc` because
    /// `jsonschema::JSONSchema` is not `Clone`, but `Schema` is shared between
    /// the config store and the daemon `AppState`.
    validators: Vec<(String, Arc<jsonschema::JSONSchema>)>,
    /// Raw field descriptors for the frontend renderer, keyed the same way.
    descriptors: Vec<(String, SchemaDescriptor)>,
    permissive: bool,
}

impl Schema {
    /// Empty, permissive schema — accepts anything. For tests and first boot.
    pub fn default_permissive() -> Self {
        Self {
            validators: Vec::new(),
            descriptors: Vec::new(),
            permissive: true,
        }
    }

    /// Load every `*.schema.json` under `dir` and compile them.
    pub fn load_dir(dir: &Path) -> Result<Self, SchemaError> {
        let mut out = Self {
            validators: Vec::new(),
            descriptors: Vec::new(),
            permissive: false,
        };
        if !dir.exists() {
            tracing::warn!("schema dir {:?} missing — permissive mode", dir);
            out.permissive = true;
            return Ok(out);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .trim_end_matches(".schema")
                .to_string();
            let raw = std::fs::read_to_string(&path)?;
            let value: Value = serde_json::from_str(&raw)?;
            // The descriptor (for the frontend) is the same document; we also
            // compile a JSON-Schema validator from the `validate` subschema if
            // present, else from the whole document.
            let descriptor: SchemaDescriptor = serde_json::from_value(value.clone())?;
            let schema_doc = descriptor.validate.clone().unwrap_or(value);
            let validator = jsonschema::JSONSchema::compile(&schema_doc)
                .map_err(|e| SchemaError::Validation(format!("compile {stem}: {e}")))?;
            out.validators.push((stem.clone(), Arc::new(validator)));
            out.descriptors.push((stem, descriptor));
        }
        Ok(out)
    }

    /// Validate a config tree against all loaded validators. Permissive mode
    /// accepts anything.
    pub fn validate(&self, config: &Config) -> Result<(), SchemaError> {
        if self.permissive {
            return Ok(());
        }
        let whole = serde_json::to_value(config).map_err(SchemaError::Parse)?;
        for (name, validator) in &self.validators {
            let target = if name.starts_with("app:") {
                whole.get("app_settings").cloned().unwrap_or(Value::Null)
            } else {
                whole.get(name.as_str()).cloned().unwrap_or(Value::Null)
            };
            if target.is_null() {
                continue;
            }
            // `is_valid` returns a plain bool — `validate` returns an iterator
            // that borrows `target`, and that borrow outlives the temporary,
            // so `is_valid` avoids the lifetime entanglement here (spec §3).
            if !validator.is_valid(&target) {
                return Err(SchemaError::Validation(format!(
                    "section {name}: validation failed"
                )));
            }
        }
        Ok(())
    }

    /// All descriptors, for the frontend renderer API.
    pub fn descriptors(&self) -> &[(String, SchemaDescriptor)] {
        &self.descriptors
    }
}

/// A schema descriptor — the unit the frontend renders into a form (spec §3).
/// This is the on-disk format other Yocto recipes drop into `schemas/`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaDescriptor {
    /// Section id, e.g. "system", "network", "app:customer-app".
    pub id: String,
    /// Human-readable title.
    #[serde(default)]
    pub title: String,
    /// Logical field groups for layout.
    #[serde(default)]
    pub groups: Vec<SchemaGroup>,
    /// Optional embedded JSON Schema used for server-side validation. When
    /// absent, the whole descriptor is treated as the validator (render-only
    /// schemas may omit it).
    #[serde(default)]
    pub validate: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaGroup {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub fields: Vec<SchemaField>,
}

/// One renderable field. The `field_type` set is fixed (spec §3): string, int,
/// bool, enum, secret.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaField {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// string | int | bool | enum | secret
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    /// Enum choices (only for `enum`).
    #[serde(default)]
    pub options: Vec<EnumOption>,
    /// Validation rules.
    #[serde(default)]
    pub validation: ValidationRules,
    /// Help text shown under the field.
    #[serde(default)]
    pub help: String,
    /// Declares a side effect to run after apply (spec §3 `apply_hook`):
    /// "restart unit X" or "sighup pid of X".
    #[serde(default)]
    pub apply_hook: Option<ApplyHook>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnumOption {
    pub value: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationRules {
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub min_len: Option<usize>,
    #[serde(default)]
    pub max_len: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplyHook {
    /// `restart` | `sighup` | `reload`.
    pub kind: String,
    /// A systemd unit name (for restart/reload) or a process name (for sighup).
    pub target: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_schema_accepts_anything() {
        let s = Schema::default_permissive();
        s.validate(&Config::default()).unwrap();
    }

    #[test]
    fn missing_dir_falls_back_to_permissive() {
        let s = Schema::load_dir(Path::new("/no/such/dir/here")).unwrap();
        s.validate(&Config::default()).unwrap();
    }
}