//! OpenUI Lang host for the Omni desktop: the backend owns evaluation
//! (deterministic, bounded), the webview renders the evaluated tree.
//!
//! Agents emit OpenUI Lang in a fenced ```omni-ui block. The frontend calls
//! `evaluate_openui`; this module parses against the Omni component library,
//! evaluates with hard limits, and returns the tree as JSON for the React
//! renderer. Actions come back as plans for the client to dispatch — nothing
//! here executes model-authored code.

use openui::{ComponentDef, LibraryBuilder, Param, Type};
use serde_json::{json, Value};

/// The Omni component library: the genUI catalog as OpenUI Lang components.
/// Param types drive model-facing validation and the client-facing render.
fn omni_library() -> Result<openui::Library, String> {
    let b = LibraryBuilder::default()
        .component(
            ComponentDef::new("Text", "A run of text")
                .param(Param::required("text", Type::String)),
        )
        .component(
            ComponentDef::new("ContactCard", "A person's card with presence")
                .param(Param::required("name", Type::String))
                .param(Param::optional("title", Type::String))
                .param(Param::optional("org", Type::String))
                .param(Param::optional("pubkey", Type::String))
                .param(Param::optional("status", Type::String))
                .param(Param::optional("tags", Type::list(Type::String)))
                .param(Param::optional("notes", Type::String)),
        )
        .component(
            ComponentDef::new("MetricCard", "A number that matters, with trend")
                .param(Param::required("label", Type::String))
                .param(Param::required("value", Type::String))
                .param(Param::optional("unit", Type::String))
                .param(Param::optional("delta", Type::String))
                .param(Param::optional("sparkline", Type::list(Type::Number)))
                .param(Param::optional("source", Type::String)),
        )
        .component(
            ComponentDef::new("Action", "One button; label is shown, prompt is dispatched to the agent")
                .param(Param::required("label", Type::String))
                .param(Param::optional("prompt", Type::String)),
        )
        .component(
            ComponentDef::new("Actions", "Action buttons row")
                .param(Param::required("actions", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("StatusGrid", "Service health at a glance")
                .param(Param::required("title", Type::String))
                .param(Param::required("services", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("Status", "One service's status")
                .param(Param::required("name", Type::String))
                .param(Param::required("status", Type::String))
                .param(Param::optional("detail", Type::String)),
        )
        .component(
            ComponentDef::new("Progress", "Completion of a task")
                .param(Param::required("label", Type::String))
                .param(Param::required("percent", Type::Number))
                .param(Param::optional("detail", Type::String)),
        )
        .component(
            ComponentDef::new("Checklist", "Tickable to-do list")
                .param(Param::required("title", Type::String))
                .param(Param::required("items", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("CheckItem", "One checklist entry")
                .param(Param::required("text", Type::String))
                .param(Param::optional("done", Type::Boolean)),
        )
        .component(
            ComponentDef::new("Table", "Structured data grid")
                .param(Param::required("title", Type::String))
                .param(Param::required("columns", Type::list(Type::component())))
                .param(Param::required("rows", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("Column", "A table column: key maps into rows")
                .param(Param::required("key", Type::String))
                .param(Param::required("label", Type::String)),
        )
        .component(
            ComponentDef::new("Row", "A table row: fields by column key")
                .param(Param::required("values", Type::String)),
        )
        .component(
            ComponentDef::new("Timeline", "Ordered events")
                .param(Param::required("title", Type::String))
                .param(Param::required("entries", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("Entry", "One timeline entry")
                .param(Param::required("title", Type::String))
                .param(Param::optional("time", Type::String))
                .param(Param::optional("detail", Type::String)),
        )
        .component(
            ComponentDef::new("KpiGrid", "Key numbers in a grid")
                .param(Param::required("metrics", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("Kpi", "One KPI")
                .param(Param::required("label", Type::String))
                .param(Param::required("value", Type::String))
                .param(Param::optional("unit", Type::String))
                .param(Param::optional("delta", Type::String)),
        )
        .component(
            ComponentDef::new("BarChart", "Comparisons across categories")
                .param(Param::required("title", Type::String))
                .param(Param::required("bars", Type::list(Type::component())))
                .param(Param::optional("unit", Type::String)),
        )
        .component(
            ComponentDef::new("Bar", "One bar")
                .param(Param::required("label", Type::String))
                .param(Param::required("value", Type::Number)),
        )
        .component(
            ComponentDef::new("Alert", "Callout: info/warn/error/success")
                .param(Param::required("level", Type::String))
                .param(Param::required("message", Type::String))
                .param(Param::optional("title", Type::String)),
        )
        .component(
            ComponentDef::new("Poll", "Options with vote percentages")
                .param(Param::required("question", Type::String))
                .param(Param::required("options", Type::list(Type::component()))),
        )
        .component(
            ComponentDef::new("PollOption", "One poll option with its vote count")
                .param(Param::required("label", Type::String))
                .param(Param::optional("votes", Type::Number)),
        )
        .component(
            ComponentDef::new("ItemCard", "A product, repo, or release card")
                .param(Param::required("title", Type::String))
                .param(Param::optional("subtitle", Type::String))
                .param(Param::optional("status", Type::String))
                .param(Param::optional("tags", Type::list(Type::String)))
                .param(Param::optional("notes", Type::String)),
        )
        .component(
            ComponentDef::new("Field", "A label/value pair")
                .param(Param::required("label", Type::String))
                .param(Param::required("value", Type::String)),
        )
        .component(
            ComponentDef::new("Rive", "A Rive asset with view-model bindings (data only)")
                .param(Param::required("asset", Type::String))
                .param(Param::optional("stateMachine", Type::String))
                .param(Param::optional("caption", Type::String)),
        )
        .root("ContactCard")
        .build()
        .map_err(|e| format!("omni library: {e}"))?;
    Ok(b)
}

/// Serialize an evaluated element into the JSON shape the React renderer
/// walks: {component, props:{name: value|tree}}.
fn element_to_json(el: &openui::Element) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("component".into(), json!(el.component()));
    let mut props_json = serde_json::Map::new();
    for prop in el.props() {
        let value = value_to_json(prop.value());
        props_json.insert(prop.name().to_string(), value);
    }
    out.insert("props".into(), Value::Object(props_json));
    Value::Object(out)
}

fn value_to_json(v: &openui::Value) -> Value {
    if let openui::Value::Element(el) = v {
        return element_to_json(el);
    }
    match v {
        openui::Value::Null => Value::Null,
        openui::Value::Bool(b) => json!(b),
        openui::Value::Number(n) => json!(n),
        openui::Value::String(s) => json!(s),
        openui::Value::Array(items) => {
            let arr: Vec<Value> = items.iter().map(value_to_json).collect();
            json!(arr)
        }
        openui::Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in map.iter() {
                obj.insert(k.to_string(), value_to_json(val));
            }
            Value::Object(obj)
        }
        openui::Value::Action(_) => json!({"action": true}),
        // handled by the guard above
        openui::Value::Element(el) => element_to_json(el),
    }
}

/// Evaluate an OpenUI Lang document against the Omni library.
/// Returns `{ok, root?, diagnostics[]}` — the client renders `root`; the
/// host never executes model-authored code.
#[tauri::command]
pub fn evaluate_openui(code: String) -> Result<Value, String> {
    let library = omni_library()?;
    let document = openui::parse_with_limits(&library, &code, openui::Limits::DEFAULT);

    let diagnostics: Vec<Value> = document
        .diagnostics()
        .iter()
        .map(|d| {
            json!({
                "code": d.code.to_string(),
                "severity": format!("{:?}", d.severity),
                "message": d.message.to_string(),
            })
        })
        .collect();

    let evaluation = document.evaluate(&openui::Inputs::new());
    let root = evaluation.root().map(element_to_json);

    Ok(json!({
        "ok": root.is_some(),
        "root": root,
        "diagnostics": diagnostics,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_a_metric_card_program() {
        let program = r#"
root = MetricCard(label: "Graph edges", value: "208", unit: "edges",
                  sparkline: [190, 196, 201, 208], source: "mneme-cache /health")
"#;
        let result = evaluate_openui(program.to_string()).unwrap();
        assert_eq!(result["ok"], json!(true));
        assert_eq!(result["root"]["component"], json!("MetricCard"));
        assert_eq!(result["root"]["props"]["label"], json!("Graph edges"));
        assert_eq!(result["root"]["props"]["value"], json!("208"));
        assert_eq!(
            result["root"]["props"]["sparkline"],
            json!([190.0, 196.0, 201.0, 208.0])
        );
    }

    #[test]
    fn rejects_unknown_components() {
        let program = r#"
root = EvilFrame(src: "https://evil.example")
"#;
        let result = evaluate_openui(program.to_string()).unwrap();
        assert_eq!(result["ok"], json!(false));
        assert!(!result["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn rejects_garbage_with_diagnostics() {
        let result = evaluate_openui("this is not openui at all %%".to_string()).unwrap();
        assert_eq!(result["ok"], json!(false));
    }
}
