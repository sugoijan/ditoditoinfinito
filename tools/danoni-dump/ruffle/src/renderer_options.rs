//! Renderer policy shared by the trusted host and the confined worker.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    #[default]
    Auto,
    Metal,
    Vulkan,
    Dx12,
    Gl,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Adapter {
    #[default]
    Auto,
    Hardware,
    Software,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct RendererOptions {
    pub backend: Backend,
    pub adapter: Adapter,
    /// Exact adapter name, never a driver path or an environment override.
    pub adapter_name: Option<String>,
}
impl RendererOptions {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(name) = &self.adapter_name
            && (name.is_empty() || name.len() > 512 || name.chars().any(char::is_control))
        {
            return Err("Adapter name must contain 1..512 bytes without control characters");
        }
        Ok(())
    }
    /// Validate the selected device against the immutable requested policy.
    pub fn validate_report(&self, report: &serde_json::Value) -> Result<(), &'static str> {
        self.validate()?;
        if report["schema"] != "ddi-ruffle-renderer/1"
            || report["requested"]
                != serde_json::to_value(self).map_err(|_| "Renderer policy serialization")?
            || report["attempts"].as_array().is_none_or(|a| a.len() > 36)
        {
            return Err("Renderer report identity or attempts differ");
        }
        let selected = &report["selected"];
        let backend: Backend = serde_json::from_value(selected["backend"].clone())
            .map_err(|_| "Unknown selected renderer backend")?;
        if backend == Backend::Auto || (self.backend != Backend::Auto && self.backend != backend) {
            return Err("Selected renderer backend differs from request");
        }
        let name = selected["name"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or("Missing adapter name")?;
        if self
            .adapter_name
            .as_ref()
            .is_some_and(|wanted| wanted != name)
        {
            return Err("Selected adapter name differs from request");
        }
        let device = selected["deviceType"]
            .as_str()
            .ok_or("Missing adapter device type")?;
        if !["Cpu", "IntegratedGpu", "DiscreteGpu", "VirtualGpu", "Other"].contains(&device)
            || selected["software"].as_bool() != Some(device == "Cpu")
            || !self.accepts_cpu(device == "Cpu")
        {
            return Err("Selected adapter type differs from request");
        }
        Ok(())
    }
    pub fn accepts_cpu(&self, cpu: bool) -> bool {
        match self.adapter {
            Adapter::Auto => true,
            Adapter::Hardware => !cpu,
            Adapter::Software => cpu,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn policy_is_closed_and_software_never_selects_hardware() {
        let default: RendererOptions = serde_json::from_value(json!({})).unwrap();
        assert_eq!(default, RendererOptions::default());
        for bad in [
            json!({"backend":"cuda"}),
            json!({"adapter":"any"}),
            json!({"driverPath":"/tmp/a"}),
        ] {
            assert!(serde_json::from_value::<RendererOptions>(bad).is_err());
        }
        for adapter in [Adapter::Auto, Adapter::Hardware, Adapter::Software] {
            let options = RendererOptions {
                adapter,
                ..default.clone()
            };
            assert_eq!(options.accepts_cpu(true), adapter != Adapter::Hardware);
            assert_eq!(options.accepts_cpu(false), adapter != Adapter::Software);
        }
        for name in ["".into(), "a\nb".into(), "a".repeat(513)] {
            assert!(
                RendererOptions {
                    adapter_name: Some(name),
                    ..default.clone()
                }
                .validate()
                .is_err()
            );
        }
        RendererOptions {
            adapter_name: Some("Example GPU".into()),
            ..default
        }
        .validate()
        .unwrap();
    }
    #[test]
    fn selected_device_receipt_is_bound_to_policy() {
        let options = RendererOptions {
            backend: Backend::Vulkan,
            adapter: Adapter::Software,
            adapter_name: Some("CPU rasterizer".into()),
        };
        let report = json!({"schema":"ddi-ruffle-renderer/1","requested":options,"selected":{"backend":"vulkan","name":"CPU rasterizer","deviceType":"Cpu","software":true},"attempts":[]});
        options.validate_report(&report).unwrap();
        for (key, value) in [
            ("backend", json!("metal")),
            ("name", json!("Other")),
            ("deviceType", json!("DiscreteGpu")),
            ("software", json!(false)),
        ] {
            let mut bad = report.clone();
            bad["selected"][key] = value;
            assert!(options.validate_report(&bad).is_err(), "{key}");
        }
        let mut bad = report;
        bad["requested"]["adapter"] = json!("auto");
        assert!(options.validate_report(&bad).is_err());
    }
}
