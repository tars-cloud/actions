use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(super) fn check(root: &Path, action: &str) -> Result<()> {
    let file = root.join(action).join("example.yaml");
    let source = fs::read_to_string(&file)
        .with_context(|| format!("every action needs {}", file.display()))?;
    let workflow: Value = serde_norway::from_str(&source)?;
    ensure!(
        source.starts_with("---\n"),
        "example must start with ---: {action}"
    );
    ensure!(
        workflow.get("on").is_some(),
        "example must be a complete workflow: {action}"
    );
    let jobs = workflow["jobs"]
        .as_object()
        .context("example workflow jobs")?;
    let owner = action.split("/scripts/").next().context("action owner")?;
    let mut exercises_owner = false;
    for job in jobs.values() {
        let steps = job["steps"].as_array().context("example job steps")?;
        let mut targets = BTreeMap::new();
        for step in steps {
            let id = step["id"].as_str().context("example step id")?;
            ensure!(step["name"].is_string(), "example step needs a name: {id}");
            if let Some(reference) = step["uses"].as_str() {
                if let Some(reference) = reference.strip_prefix("tars-cloud/actions/") {
                    let (target, version) = reference
                        .split_once('@')
                        .context("example action revision")?;
                    ensure!(
                        target
                            .strip_prefix("composite/")
                            .is_some_and(|name| !name.is_empty()
                                && name.chars().all(|c| c.is_ascii_lowercase() || c == '-')),
                        "examples must use public actions: {reference}"
                    );
                    ensure!(
                        version == "v1"
                            || (version.len() == 40
                                && version.chars().all(|c| c.is_ascii_hexdigit())),
                        "example needs the next release alias or a reviewed full SHA"
                    );
                    let metadata: Value = serde_norway::from_str(&fs::read_to_string(
                        root.join(target).join("action.yml"),
                    )?)?;
                    validate_inputs(step, &metadata)
                        .with_context(|| format!("{action}/example.yaml step {id}"))?;
                    exercises_owner |= target == owner;
                    ensure!(
                        targets.insert(id, metadata).is_none(),
                        "duplicate example step id: {id}"
                    );
                } else {
                    ensure!(
                        super::metadata::reviewed(reference),
                        "unreviewed example action: {reference}"
                    );
                }
            }
            for run in [step["run"].as_str(), step["with"]["run"].as_str()]
                .into_iter()
                .flatten()
            {
                ensure!(
                    !run.contains("${{"),
                    "pass expression values through env in example step {id}"
                );
            }
        }
        let text = serde_json::to_string(job)?;
        for reference in text.split("steps.").skip(1) {
            let id = identifier(reference);
            if let Some(rest) = reference[id.len()..].strip_prefix(".outputs.")
                && let Some(metadata) = targets.get(id)
            {
                let output = identifier(rest);
                ensure!(
                    metadata["outputs"].get(output).is_some(),
                    "unknown example output: {id}.{output}"
                );
            }
        }
    }
    ensure!(
        exercises_owner,
        "example does not use its public action: {action}"
    );
    Ok(())
}

fn identifier(value: &str) -> &str {
    let end = value
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(value.len());
    &value[..end]
}

fn validate_inputs(step: &Value, metadata: &Value) -> Result<()> {
    if let Some(inputs) = step["with"].as_object() {
        for input in inputs.keys() {
            ensure!(
                metadata["inputs"].get(input).is_some(),
                "unknown example input: {input}"
            );
        }
    }
    if let Some(inputs) = metadata["inputs"].as_object() {
        for (name, input) in inputs {
            if input["required"] == true && input.get("default").is_none() {
                ensure!(
                    step["with"]
                        .get(name)
                        .is_some_and(|value| value.as_str().is_some_and(|value| !value.is_empty())),
                    "missing required example input: {name}"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn examples_reject_unknown_and_missing_inputs() {
        let metadata = json!({"inputs":{"run":{"required":true},"system":{"default":""}}});
        assert!(
            validate_inputs(
                &json!({"with":{"run":"true","system":"aarch64-linux"}}),
                &metadata
            )
            .is_ok()
        );
        for inputs in [
            json!({}),
            json!({"run":""}),
            json!({"run":"true","sytem":"aarch64-linux"}),
        ] {
            assert!(validate_inputs(&json!({"with":inputs}), &metadata).is_err());
        }
    }

    #[test]
    fn example_contract_detects_removed_outputs_and_missing_examples() -> Result<()> {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tars/scratch/examples");
        fs::create_dir_all(&scratch)?;
        let root = tempfile::tempdir_in(scratch)?;
        let action = root.path().join("composite/sample");
        fs::create_dir_all(&action)?;
        assert!(check(root.path(), "composite/sample").is_err());
        fs::write(action.join("action.yml"), "---\noutputs:\n  version: {}\n")?;
        let workflow = json!({"on":{"workflow_dispatch":{}},"jobs":{"sample":{"steps":[
            {"id":"setup","name":"Prepare","uses":"tars-cloud/actions/composite/sample@v1"},
            {"id":"report","name":"Report","env":{"VERSION":"${{ steps.setup.outputs.version }}"},"run":"echo \"$VERSION\""}
        ]}}});
        fs::write(
            action.join("example.yaml"),
            format!("---\n{}", serde_norway::to_string(&workflow)?),
        )?;
        check(root.path(), "composite/sample")?;
        fs::write(action.join("action.yml"), "---\noutputs: {}\n")?;
        assert!(check(root.path(), "composite/sample").is_err());
        Ok(())
    }
}
