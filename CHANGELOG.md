<!-- markdownlint-configure-file {"MD024": {"siblings_only": true}} -->

# Changelog

## [v3.7.0](https://github.com/tars-cloud/actions/compare/v3.6.1...13358a7689dbec76f5d7e4cbd4261422ca95712e) (2026-10-19)

### Features

- more CRAP (#139)
  ([e82271b](https://github.com/tars-cloud/actions/commit/e82271b5ffda60a46ec7cb1f5f60fec9e77e2986)),
  closes [#139](https://github.com/tars-cloud/actions/issues/139)

### Dependencies

- **deps:** bump the github-actions-deps group across 1 directory with 4
  updates (#142)
  ([13358a7](https://github.com/tars-cloud/actions/commit/13358a7689dbec76f5d7e4cbd4261422ca95712e)),
  closes [#142](https://github.com/tars-cloud/actions/issues/142)

### [v3.6.1](https://github.com/tars-cloud/actions/compare/v3.6.0...v3.6.1) (2026-10-19)

#### Dependencies

- **deps:** bump toml_edit (#129)
  ([8cbeea3](https://github.com/tars-cloud/actions/commit/8cbeea3954968187ee9a16dbb6e0bb266a6a6f16)),
  closes [#129](https://github.com/tars-cloud/actions/issues/129)
- **deps:** bump the github-actions-deps group across 2 directories with 6
  updates (#130)
  ([d3996e5](https://github.com/tars-cloud/actions/commit/d3996e5c86878aaeee76dce0e396b6f1faac4dcd)),
  closes [#130](https://github.com/tars-cloud/actions/issues/130)
- **deps:** bump serde_json from 1.0.151 to 1.0.154 (#128)
  ([ee9cd35](https://github.com/tars-cloud/actions/commit/ee9cd3582cf170ad5f1b0c34f398adaab9532bb1)),
  closes [#128](https://github.com/tars-cloud/actions/issues/128)

## [v3.6.0](https://github.com/tars-cloud/actions/compare/v3.5.0...v3.6.0) (2026-10-17)

### Features

- add LLVM merge policy and opt-in hosted cleanup (#122)
  ([7687ad9](https://github.com/tars-cloud/actions/commit/7687ad994534776e779259ff8c1b188a1a52a1f7)),
  closes [#122](https://github.com/tars-cloud/actions/issues/122)

## [v3.5.0](https://github.com/tars-cloud/actions/compare/v3.4.0...v3.5.0) (2026-10-17)

### Features

- **crap:** support merge queues and strict bootstrap (#116)
  ([a9f4681](https://github.com/tars-cloud/actions/commit/a9f4681eff594631128f339bbb5e978e88aa7efa)),
  closes [#116](https://github.com/tars-cloud/actions/issues/116)

## [v3.4.0](https://github.com/tars-cloud/actions/compare/v3.3.0...v3.4.0) (2026-10-16)

### Features

- **crap:** warn and pass on unbootstrapped initial runs (#105)
  ([1aea74f](https://github.com/tars-cloud/actions/commit/1aea74ff5b8b5e63f7c62b8695e07ac86435623f)),
  closes [#105](https://github.com/tars-cloud/actions/issues/105)

### Dependencies

- **deps:** bump the github-actions-deps group across 1 directory with 2
  updates (#104)
  ([112c5b8](https://github.com/tars-cloud/actions/commit/112c5b831c6c116cf9b2a037df0e8f4cd5001c9b)),
  closes [#104](https://github.com/tars-cloud/actions/issues/104)
- **deps:** bump jsonschema from 0.58.5 to 0.58.6 (#103)
  ([6050c3a](https://github.com/tars-cloud/actions/commit/6050c3a03a600bb4fffe01302a9d522d16499a9c)),
  closes [#103](https://github.com/tars-cloud/actions/issues/103)

## [v3.3.0](https://github.com/tars-cloud/actions/compare/v3.2.0...v3.3.0) (2026-10-08)

### Features

- **ci:** add deterministic TARS Cloud runner selection (#90)
  ([2762a73](https://github.com/tars-cloud/actions/commit/2762a73affc51ae63fc5ac5d9d0a4304488b7be0)),
  closes [#90](https://github.com/tars-cloud/actions/issues/90)

### Fixes

- **tact:** kill timed-out children before reaping (#97)
  ([291024c](https://github.com/tars-cloud/actions/commit/291024ce5dd84c7d52fe00f777241a0870632242)),
  closes [#97](https://github.com/tars-cloud/actions/issues/97)
- **ci:** restrict Devenv PR probe to numeric fixture branches (#95)
  ([a172761](https://github.com/tars-cloud/actions/commit/a172761ded1f9e18b60c1c2783e278521a251f5a)),
  closes [#95](https://github.com/tars-cloud/actions/issues/95)

## [v3.2.0](https://github.com/tars-cloud/actions/compare/v3.1.0...v3.2.0) (2026-10-05)

### Features

- **ci:** add shared consumer setup and runner controls (#82)
  ([fda9bd4](https://github.com/tars-cloud/actions/commit/fda9bd4ce72b81d9bfb90d9597bb647b90a49f64)),
  closes [#82](https://github.com/tars-cloud/actions/issues/82)

## [v3.1.0](https://github.com/tars-cloud/actions/compare/v3.0.1...v3.1.0) (2026-10-04)

### Features

- **cargo-crap:** gate thresholds and warn on regressions (#56)
  ([3f76e31](https://github.com/tars-cloud/actions/commit/3f76e3114a59814dccd750f6687e35c95c331f8c)),
  closes [#56](https://github.com/tars-cloud/actions/issues/56)
- **cargo-crap:** add reusable coverage and regression CI (#53)
  ([fab732b](https://github.com/tars-cloud/actions/commit/fab732beb6d890f4e0314058bc21c0ff54485344)),
  closes [#53](https://github.com/tars-cloud/actions/issues/53)

### Fixes

- **ci:** repair release lockfiles and clean CRAP commit messages (#77)
  ([ef30332](https://github.com/tars-cloud/actions/commit/ef30332f72871a4be25882af6e56f5d8ea93cdea)),
  closes [#77](https://github.com/tars-cloud/actions/issues/77)
- **cargo-crap:** queue publishers and distinguish fixture PRs (#73)
  ([f0c7093](https://github.com/tars-cloud/actions/commit/f0c709361d0f0bd4ed8747eecff60924efb942c4)),
  closes [#73](https://github.com/tars-cloud/actions/issues/73)
- **ci:** respect repository merge methods in CRAP lifecycle (#70)
  ([fec6c65](https://github.com/tars-cloud/actions/commit/fec6c65b657f1a53f55c8922c40328c5918a47d6)),
  closes [#70](https://github.com/tars-cloud/actions/issues/70)
- **cargo-crap:** recognise GitHub-signed App commits (#66)
  ([3091207](https://github.com/tars-cloud/actions/commit/3091207673b9eb51d2723dfc70a61dfb69662107)),
  closes [#66](https://github.com/tars-cloud/actions/issues/66)
- **ci:** authenticate CRAP lifecycle with the CI App (#62)
  ([3cca810](https://github.com/tars-cloud/actions/commit/3cca8108326825adfae8ac06f3a1c7e692e6cc68)),
  closes [#62](https://github.com/tars-cloud/actions/issues/62)
- **ci:** make the CodeQL workflow timeout configurable (#51)
  ([4359380](https://github.com/tars-cloud/actions/commit/43593806caaf40b5d2f86a52c6f2fb26bcc37d2d)),
  closes [#51](https://github.com/tars-cloud/actions/issues/51)
- **ci:** make the Trivy workflow timeout configurable (#50)
  ([f6a6010](https://github.com/tars-cloud/actions/commit/f6a6010b4d78b9a7517f82624ff954b085470c69)),
  closes [#50](https://github.com/tars-cloud/actions/issues/50)

### Dependencies

- **deps:** bump jsonschema from 0.58.2 to 0.58.3 (#54)
  ([e8aafef](https://github.com/tars-cloud/actions/commit/e8aafeff8f8e6209309e01313d64a036df5cda61)),
  closes [#54](https://github.com/tars-cloud/actions/issues/54)

### [v3.0.1](https://github.com/tars-cloud/actions/compare/v3.0.0...v3.0.1) (2026-09-30)

#### Fixes

- **ci:** distinguish matrix variant job names (#46)
  ([5bd72ab](https://github.com/tars-cloud/actions/commit/5bd72ab1fe270ea7c27272baf958f28b0f14adef)),
  closes [#46](https://github.com/tars-cloud/actions/issues/46)

## [v3.0.0](https://github.com/tars-cloud/actions/compare/v2.3.0...v3.0.0) (2026-09-30)

### ⚠ BREAKING CHANGE

- Reusable workflow paths use purpose-prefixed .yaml names.
  Composite metadata files now use action.yaml. Update consumer calls for v3.

### Features

- standardize workflow naming and YAML style (#32)
  ([98e54e6](https://github.com/tars-cloud/actions/commit/98e54e66c696f94c929ee1bc14154ac4d26edf55)),
  closes [#32](https://github.com/tars-cloud/actions/issues/32)
- add reusable devenv dependency update workflow (#28)
  ([e7addd7](https://github.com/tars-cloud/actions/commit/e7addd7f0d08737471f8627199daf6cbdb3be639)),
  closes [#28](https://github.com/tars-cloud/actions/issues/28)

### Fixes

- **ci:** match update probe by workflow path (#39)
  ([a71b4f4](https://github.com/tars-cloud/actions/commit/a71b4f48d5375b4968ed484efefc544daf0c2f29)),
  closes [#39](https://github.com/tars-cloud/actions/issues/39)
- **devenv:** support clean consumers and nixpkgs Convco (#27)
  ([889dfeb](https://github.com/tars-cloud/actions/commit/889dfeb06283fa5142ef72437136e6ac8f3291d0)),
  closes [#27](https://github.com/tars-cloud/actions/issues/27)

### Dependencies

- **deps:** bump peter-evans/create-pull-request (#36)
  ([61249df](https://github.com/tars-cloud/actions/commit/61249dfde45990c19612851b1dc99be8c32fc26c)),
  closes [#36](https://github.com/tars-cloud/actions/issues/36)
- **deps:** bump jsonschema from 0.57.0 to 0.58.0 (#34)
  ([5c38dcb](https://github.com/tars-cloud/actions/commit/5c38dcbe4b98c0f5d7d2521af2674cf2949f5960)),
  closes [#34](https://github.com/tars-cloud/actions/issues/34)

## [v2.3.0](https://github.com/tars-cloud/actions/compare/v2.2.0...v2.3.0) (2026-09-28)

### Features

- **release-rust:** add reusable Cargo release lifecycle (#24)
  ([7bd083a](https://github.com/tars-cloud/actions/commit/7bd083a781a71264d2997df06a533d4bb4e722a4)),
  closes [#24](https://github.com/tars-cloud/actions/issues/24)

### Fixes

- **release-rust:** correct repository metadata API endpoint (#25)
  ([ce792e6](https://github.com/tars-cloud/actions/commit/ce792e6556fc1921833f225aee35e1c2e4f833e3)),
  closes [#25](https://github.com/tars-cloud/actions/issues/25)

## [v2.2.0](https://github.com/tars-cloud/actions/compare/v2.1.0...v2.2.0) (2026-09-27)

### Features

- **devenv:** support CodeQL toolchains and efficient emulation (#22)
  ([42454d8](https://github.com/tars-cloud/actions/commit/42454d873937196488a9304b043ca76152d0ab68)),
  closes [#22](https://github.com/tars-cloud/actions/issues/22)

## [v2.1.0](https://github.com/tars-cloud/actions/compare/v2.0.0...v2.1.0) (2026-09-27)

### Features

- **release:** automate preparation of a single release PR (#19)
  ([8e83e1b](https://github.com/tars-cloud/actions/commit/8e83e1bb3af94deebfe43779750ecaeee604ce61)),
  closes [#19](https://github.com/tars-cloud/actions/issues/19)

### Fixes

- **ci:** give workflow jobs distinct readable names (#21)
  ([bff0c0f](https://github.com/tars-cloud/actions/commit/bff0c0fe1f698815a09e914c66318813d4701636)),
  closes [#21](https://github.com/tars-cloud/actions/issues/21)

## [v2.0.0](https://github.com/tars-cloud/actions/compare/v1.1.0...v2.0.0) (2026-09-27)

### ⚠ BREAKING CHANGE

- configure Cachix in the consumer devenv environment; the Nix cache composite action and its inputs are removed.

- feat(security): add reusable Trivy and CodeQL workflows

- fix(tact): include workflow examples in the Nix source

### Features

- **security:** add shared scans and remove composite Cachix (#15)
  ([8916e7c](https://github.com/tars-cloud/actions/commit/8916e7c5c876db9b5722b5dd807fea40a5f5267e)),
  closes [#15](https://github.com/tars-cloud/actions/issues/15)

### Fixes

- **ci:** allow Dependabot pin updates and absent results (#17)
  ([9e12677](https://github.com/tars-cloud/actions/commit/9e126777266b75d894b6d74ff97c2456e70f7745)),
  closes [#17](https://github.com/tars-cloud/actions/issues/17)

### Dependencies

- **deps:** bump the github-actions-deps group across 1 directory with 4
  updates (#16)
  ([1dd8190](https://github.com/tars-cloud/actions/commit/1dd81904176dc1249f97f18656d949d789994d7f)),
  closes [#16](https://github.com/tars-cloud/actions/issues/16)

## [v1.1.0](https://github.com/tars-cloud/actions/compare/v1.0.0...v1.1.0) (2026-09-25)

### Features

- **run-devenv:** expose optional structured JSON results (#13)
  ([c83869e](https://github.com/tars-cloud/actions/commit/c83869ed634aa33050cc74e3499f6185301d69bf)),
  closes [#13](https://github.com/tars-cloud/actions/issues/13)

## [v1.0.0](https://github.com/tars-cloud/actions/compare/v0.1.0...v1.0.0) (2026-09-25)

### ⚠ BREAKING CHANGE

- move cachix-name and cachix-token inputs, and references to cachix-mode, from setup-cache to setup-nix-cache.

- fix(ci): capture expected failures without error annotations

Label reporting smoke-test summaries as intentional fixture data and assert exit 7 inside a successful step. Run missing-Trivy checks through Tact so expected diagnostics are captured instead of annotating successful jobs.

- fix(tact): retain tool PATH in isolated reporting test

The reporting regression test cleared PATH before launching Bash, so the Nix package build could not find the executable. Preserve the declared build-tool PATH while clearing the remaining environment. Verified by reproducing the failure and rebuilding nix/packages/tact.nix successfully.

- feat: support pinned devenv execution and Cachix controls

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
