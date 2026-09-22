"""Validate public contracts and nested action wiring without network access."""

from pathlib import Path
import re
import yaml

root = Path(__file__).resolve().parent.parent
public = ["setup-nix", "setup-devenv", "setup-trivy", "setup-cache", "free-disk-space"]
reviewed = {
    "actions/cache": "55cc8345863c7cc4c66a329aec7e433d2d1c52a9",
    "runs-on/cache": "88d90644011a3a9957fd141a106f5a94f9794203",
    "cachix/install-nix-action": "13d8dd58da0234aa297dedd986986ccb8e7f3e24",
    "cachix/cachix-action": "38b082610b782e7e93e209c35fd730d399dee866",
    "actions/checkout": "3d3c42e5aac5ba805825da76410c181273ba90b1",
}


def load(name):
    return yaml.safe_load((root / name / "action.yml").read_text())


for name in public:
    action = load(name)
    assert action["runs"]["using"] == "composite", name
    assert (root / name / "README.md").is_file(), name

for file in [*(root / "internal").rglob("action.yml"), *[root / name / "action.yml" for name in public]]:
    action = yaml.safe_load(file.read_text())
    if action["runs"]["using"] == "node24":
        assert (file.parent / action["runs"]["main"]).is_file()
        continue
    for step in action["runs"]["steps"]:
        if "run" in step:
            assert step["shell"] == "bash"
            assert "${{" not in step["run"], file
        if "uses" in step:
            reference = step["uses"]
            if reference.startswith("$/"):
                target = root / reference[2:] / "action.yml"
                assert target.is_file(), target
                accepted = yaml.safe_load(target.read_text()).get("inputs", {})
                assert set(step.get("with", {})) <= set(accepted), reference
            else:
                owner, sha = reference.split("@")
                assert reviewed[owner] == sha, reference
        for value in re.findall(r"inputs\.([a-z0-9-]+)", str(step)):
            assert value in action.get("inputs", {}), (file, value)

for name in ["setup-cache", "setup-devenv"]:
    refs = [s.get("uses") for s in load(name)["runs"]["steps"]]
    assert "$/setup-nix" in refs
    assert "$/free-disk-space" not in refs
assert all("cache" not in s.get("uses", "") for s in load("setup-devenv")["runs"]["steps"])
assert all("cache" not in s.get("uses", "") for s in load("setup-trivy")["runs"]["steps"])
# Composite post hooks evaluate outputs without the main steps' JSON context.
assert "fromJSON(" not in str(load("setup-cache")["outputs"])
adapter = load("internal/cache")["runs"]["steps"]
github, s3 = adapter[:2]
assert github["if"] == "inputs.backend == 'github'"
assert s3["if"] == "inputs.backend == 's3'"
assert s3["continue-on-error"] is True
assert s3["env"]["RUNS_ON_RUNNER_NAME"] == ""
assert "AWS_ACCESS_KEY_ID" not in github.get("env", {})
assert "save-always" not in str(adapter)
ci = yaml.safe_load((root / ".github/workflows/ci.yml").read_text())
assert ci["jobs"]["cache-warm"]["needs"] == "cache-cold"
assert ci["jobs"]["flakes"]["strategy"]["matrix"]["shell"] == ["default", "named"]
for name, job in ci["jobs"].items():
    if name == "self-hosted":
        assert job["runs-on"]["group"] == "enterprise/tars-cloud"
        assert all(s.get("uses") != "./free-disk-space" for s in job["steps"])
        continue
    assert job["strategy"]["matrix"]["runner"] == ["ubuntu-24.04", "ubuntu-24.04-arm"]
print("Action metadata, immutable pins, isolation and CI lifecycle wiring passed.")
