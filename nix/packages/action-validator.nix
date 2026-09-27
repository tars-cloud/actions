{ pkgs }:
pkgs.action-validator.overrideAttrs (old: {
  # The bundled schema predates GitHub's concurrency queue option.
  postPatch = (old.postPatch or "") + ''
    schema=src/schemastore/src/schemas/json/github-workflow.json
    ${pkgs.jq}/bin/jq '
      if .definitions.concurrency.type != "object" then error("concurrency schema changed")
      else .definitions.concurrency.properties.queue = {type: "string", enum: ["single", "max"]}
      end
    ' "$schema" > "$schema.updated"
    mv "$schema.updated" "$schema"
  '';
})
