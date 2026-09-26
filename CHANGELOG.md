<!-- markdownlint-configure-file {"MD024": {"siblings_only": true}} -->

# Changelog

## [v1.1.0](https://github.com/tars-cloud/actions/compare/v1.0.0...c83869ed634aa33050cc74e3499f6185301d69bf) (2026-09-25)

### Features

- **run-devenv:** expose optional structured JSON results (#13)
  ([c83869e](https://github.com/tars-cloud/actions/commit/c83869ed634aa33050cc74e3499f6185301d69bf)),
  closes [#13](https://github.com/tars-cloud/actions/issues/13)

## [v1.0.0](https://github.com/tars-cloud/actions/compare/v0.1.0...v1.0.0) (2026-09-25)

### ⚠ BREAKING CHANGE

- fix(ci): capture expected failures without error annotations

Label reporting smoke-test summaries as intentional fixture data and assert exit 7 inside a successful step. Run missing-Trivy checks through Tact so expected diagnostics are captured instead of annotating successful jobs.

- fix(tact): retain tool PATH in isolated reporting test

The reporting regression test cleared PATH before launching Bash, so the Nix package build could not find the executable. Preserve the declared build-tool PATH while clearing the remaining environment. Verified by reproducing the failure and rebuilding nix/packages/tact.nix successfully.

- feat: support pinned devenv execution

- feat: add cache diagnostics and validated action examples

### Features

- separate caches and standardize devenv workflows (#9)
  ([7352943](https://github.com/tars-cloud/actions/commit/7352943188db88e2024371da1febcab1336ea995)),
  closes [#9](https://github.com/tars-cloud/actions/issues/9)

### Fixes

- **release:** allow repeated sections across changelog versions (#11)
  ([4919a0e](https://github.com/tars-cloud/actions/commit/4919a0e666a2a907b38ad3405d25e0f07ddc6c99)),
  closes [#11](https://github.com/tars-cloud/actions/issues/11)

### Dependencies

- **deps:** bump jsonschema from 0.56.0 to 0.57.0 (#10)
  ([dd9c881](https://github.com/tars-cloud/actions/commit/dd9c881c6dbb8ae8f68de68ae2ac7a56434195bc)),
  closes [#10](https://github.com/tars-cloud/actions/issues/10)

## v0.1.0 (2026-09-24)

### ⚠ BREAKING CHANGE

- consumers must reference tars-cloud/actions/composite/&lt;action&gt;@&lt;ref&gt;.

- feat(cache): identify consumer caches and verify live S3 lifecycle

- fix(cache): start the consumer cache format at v1

- fix(cache): keep archive paths portable across runners

- fix(cache): isolate Nix compiler changes and validate remote consumers

### Features

- **release:** fix changelog versions and publish after trunk CI (#7)
  ([5536799](https://github.com/tars-cloud/actions/commit/5536799222a347486105b8629d69c38e2bc3cbc3)),
  closes [#7](https://github.com/tars-cloud/actions/issues/7)
- add releases, status reporting and composite action layout (#3)
  ([b5d6cb9](https://github.com/tars-cloud/actions/commit/b5d6cb98dfbd99c71b18f7cefe29b80963cf8c30)),
  closes [#3](https://github.com/tars-cloud/actions/issues/3)

### Fixes

- **release:** escape HTML-like text in generated changelogs (#5)
  ([83c7bcd](https://github.com/tars-cloud/actions/commit/83c7bcdbcaaa5c1cfce85644c25a3dbdfc88606c)),
  closes [#5](https://github.com/tars-cloud/actions/issues/5)
- **release:** prepare versions without cached dependency sources (#4)
  ([c90cbda](https://github.com/tars-cloud/actions/commit/c90cbdadcde66834039906b463ba325e80a00d5b)),
  closes [#4](https://github.com/tars-cloud/actions/issues/4)

### Dependencies

- **deps:** bump nix from 0.30.1 to 0.31.3 (#2)
  ([d593846](https://github.com/tars-cloud/actions/commit/d5938466b5ade53b14db4b736c36b483a5b453c3)),
  closes [#2](https://github.com/tars-cloud/actions/issues/2)
