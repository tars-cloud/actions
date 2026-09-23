# Changelog

## 83c7bcdbcaaa5c1cfce85644c25a3dbdfc88606c (2026-09-24)

### ⚠ BREAKING CHANGE

- consumers must reference tars-cloud/actions/composite/&lt;action&gt;@&lt;ref&gt;.

- feat(cache): identify consumer caches and verify live S3 lifecycle

- fix(cache): start the consumer cache format at v1

- fix(cache): keep archive paths portable across runners

- fix(cache): isolate Nix compiler changes and validate remote consumers

### Features

- add releases, status reporting and composite action layout (#3)
  ([b5d6cb9](https://github.com/tars-cloud/actions/commit/b5d6cb98dfbd99c71b18f7cefe29b80963cf8c30)), closes
  [#3](https://github.com/tars-cloud/actions/issues/3)

### Fixes

- **release:** escape HTML-like text in generated changelogs (#5)
  ([83c7bcd](https://github.com/tars-cloud/actions/commit/83c7bcdbcaaa5c1cfce85644c25a3dbdfc88606c)), closes
  [#5](https://github.com/tars-cloud/actions/issues/5)
- **release:** prepare versions without cached dependency sources (#4)
  ([c90cbda](https://github.com/tars-cloud/actions/commit/c90cbdadcde66834039906b463ba325e80a00d5b)), closes
  [#4](https://github.com/tars-cloud/actions/issues/4)

### Dependencies

- **deps:** bump nix from 0.30.1 to 0.31.3 (#2)
  ([d593846](https://github.com/tars-cloud/actions/commit/d5938466b5ade53b14db4b736c36b483a5b453c3)), closes
  [#2](https://github.com/tars-cloud/actions/issues/2)
