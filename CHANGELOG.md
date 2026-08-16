# Changelog
## [2.0.2](https://github.com/KKamJi98/aws-profile-pick/compare/v2.0.1...v2.0.2) (2026-08-16)


### Documentation

* link back to the source from PyPI ([785a7bd](https://github.com/KKamJi98/aws-profile-pick/commit/785a7bd66936dfba3c910842101e2d104f420290))

## [2.0.1](https://github.com/KKamJi98/aws-profile-pick/compare/v2.0.0...v2.0.1) (2026-08-16)


### Bug Fixes

* **ci:** publish from the release-please job instead of a tag trigger ([bde8ef7](https://github.com/KKamJi98/aws-profile-pick/commit/bde8ef7c66a83fe10c22e04d7234790e45b867cc))

## [2.0.0](https://github.com/KKamJi98/aws-profile-pick/compare/v1.4.1...v2.0.0) (2026-08-16)


### ⚠ BREAKING CHANGES

* publish as aws-profile-pick
* rename the distribution to awspick and publish to PyPI

### refac

* publish as aws-profile-pick ([567a1ab](https://github.com/KKamJi98/aws-profile-pick/commit/567a1abf5b2eec09d5be84e3a5eeaade4b145cb0))
* rename the distribution to awspick and publish to PyPI ([dcbda7c](https://github.com/KKamJi98/aws-profile-pick/commit/dcbda7c858f80dcb5b94980b4c91d558133bf653))


### Features

* **cli:** show current AWS profile in table ([ae47974](https://github.com/KKamJi98/aws-profile-pick/commit/ae4797446e096ceea77599ac1edda9566a2188a8))
* **group:** add group dividers, others positioning, and wildcard catch-all ([2d1ca68](https://github.com/KKamJi98/aws-profile-pick/commit/2d1ca684b919775951fe1fb37eb10a5597b1d15d))
* **shell:** sync profile across shells ([537133b](https://github.com/KKamJi98/aws-profile-pick/commit/537133b96a83e62a7495f83c22899ab177c317fe))


### Bug Fixes

* apply ruff format to config.py ([4e6197f](https://github.com/KKamJi98/aws-profile-pick/commit/4e6197fa2ba3e3e5f7be624bb7a6ad7bc5fc142c))
* **backup:** keep only two rc backups ([7869b41](https://github.com/KKamJi98/aws-profile-pick/commit/7869b414bcb826cbb18ce6c752380fc152c08d42))
* **ci:** pin ruff to the project and give it a rule set ([6d83abe](https://github.com/KKamJi98/aws-profile-pick/commit/6d83abe33aa56b7b84f9f76497613cdb15c93f1b))


### Documentation

* add uv tool upgrade and editable install instructions to README.md ([78f0f07](https://github.com/KKamJi98/aws-profile-pick/commit/78f0f07ec0cf41a99f3476bc27bdbe58c2e9a333))
* replace non-ASCII dashes and point the clone URL at the new name ([e6d9961](https://github.com/KKamJi98/aws-profile-pick/commit/e6d9961f973c2b2e45574aa3de8a852f1a9f407a))
* update the project tree root name ([45dd786](https://github.com/KKamJi98/aws-profile-pick/commit/45dd786ab4c01d8516284be43be40184fe4dadb8))

## [1.4.1](https://github.com/KKamJi98/aws-pick/compare/v1.4.0...v1.4.1) (2025-12-05)


### Documentation

* document uv tool install eval flow ([8743470](https://github.com/KKamJi98/aws-pick/commit/87434703bb27bedda8f1aaf7f910b492c04a58e5))

## [1.4.0](https://github.com/KKamJi98/aws-pick/compare/v1.3.6...v1.4.0) (2025-08-13)


### Features

* add filtering and grouping options ([b5fbe11](https://github.com/KKamJi98/aws-pick/commit/b5fbe11e21cab6a57d107c00cc1c41422c677c05))
* group and color-code profiles by environment ([8192f99](https://github.com/KKamJi98/aws-pick/commit/8192f99920acc52dcd80237f4e21815c60628263))


### Bug Fixes

* add ruff to dev dependencies and remove black and isort ([d34ab6d](https://github.com/KKamJi98/aws-pick/commit/d34ab6d1e3ce64f9ac4b5bd1fe7305c7ade2ef0a))
* apply ruff fixes ([9825340](https://github.com/KKamJi98/aws-pick/commit/9825340cf371feec689e96f99d3bb8fed8f051cf))
* place preprod second from bottom and avoid prod substring matches ([03a2376](https://github.com/KKamJi98/aws-pick/commit/03a2376ff43aa1d9a29a254b3fda8dff7eb3a3b5))


### Documentation

* document default group order with preprod second from bottom ([c0a2142](https://github.com/KKamJi98/aws-pick/commit/c0a214244aa074c4155a13b766d5bd1472f07ace))
* document unified CI workflow in README ([6468223](https://github.com/KKamJi98/aws-pick/commit/646822309d03ea1fbfba898c9e0fb4e15cfe747a))
* update README with profile switching logic ([2fb1dbc](https://github.com/KKamJi98/aws-pick/commit/2fb1dbc04dee5a260dbbd48c925be6d8248ce8c0))
* update README.md to reflect new repo URL and remove pip install ([96330ac](https://github.com/KKamJi98/aws-pick/commit/96330ac63c2bc90ba26ea836abf0afe06423ba4b))
* use ruff for linting and formatting ([fa19a90](https://github.com/KKamJi98/aws-pick/commit/fa19a9001a436c4271abe483a4fd0b56205719e4))

## [1.4.0] - 2025-08-13
### Added
- Automatically group and color-code profiles by environment (dev, stg, prod, preprod).


## [1.3.6](https://github.com/KKamJi98/aws-pick/compare/v1.3.5...v1.3.6) (2025-06-28)


### Documentation

* Refactor README and CHANGELOG ([dc9576f](https://github.com/KKamJi98/aws-pick/commit/dc9576fd7598b3e0ae0427771d59eea49d7eaa48))
* Update README example and refactor GEMINI.md ([c67a6b3](https://github.com/KKamJi98/aws-pick/commit/c67a6b316f0ac43df58840b9aa0c18b2f182b053))

## [1.3.5] - 2025-06-21
### Added
- Document shell function to use `aws_pick.py` directly in README.
## [1.3.4] - 2025-06-21
### Changed
- CLI now prints menus and logs to stderr, leaving only the export command on stdout.
### Added
- Graceful exit when no selection is made.
## [1.3.3] - 2025-06-20
### Fixed
- Run tests in CI using uv virtual environment
## [1.3.2] - 2025-06-20
### Fixed
- CI workflow uses virtual environment binaries explicitly

## [1.3.1] - 2025-06-13
### Added
- Example shell wrapper function to apply profiles immediately
### Fixed
- CI test workflow activates uv virtual environment

## [1.3.0] - 2025-06-12
### Added
- GitHub Actions workflows for testing and automated releases
### Changed
- Migrate package management from Poetry to uv, update docs and environment


## [1.2.1] - 2025-06-11
### Changed
- CLI prints the export command by default; `--export-command` is no longer required

## [1.2.0] - 2025-06-11
### Added
- `--export-command` option prints the shell command to apply the profile
### Changed
- README instructions updated; shell configs are no longer sourced automatically

## [1.1.0] - 2025-06-10
### Added
- Automatically source the appropriate shell rc file after profile update.
- Limit rc file backups to the five most recent copies.

## [1.0.1] - 2025-06-09
### Added
- Support for multiple shells (bash, zsh, fish)
- Enhanced error handling and documentation

## [1.0.0] - 2025-06-05
### Functional Requirements
- `~/.aws/config` 파일에서 프로파일 목록을 읽어 번호와 이름으로 출력한다.
- 번호 또는 이름 입력을 모두 허용한다.
- 잘못된 입력은 재입력을 요구한다.
- 선택된 프로파일을 쉘 설정 파일에서 `export AWS_PROFILE` 줄로 치환하거나 추가한다.
- 원본 설정 파일을 `.bak-YYYYMMDDHHMMSS` 형식으로 백업한다.
- 같은 프로파일을 선택할 경우 파일을 수정하지 않는다.

### Non-Functional Requirements
- Python 3.9 이상.
- 외부 의존성으로 `boto3`, `tabulate` 사용.
- 입력 검증과 파일 패치 로직에 대한 100% 단위 테스트 커버리지.
- `logging` 모듈을 사용하여 INFO/ERROR 단계 로깅.
- Poetry `scripts`에 `awspick` CLI 엔트리포인트 정의.
- 프로젝트 루트에 단일 파일 런처 `aws_pick.py` 제공.

### History
- 2025-06-07 – kkamji – 에러 처리 강화 및 주석 개선
- 2025-06-06 – kkamji – `.prompt.md` GitHub 제외 명확화
- 2025-06-05 – kkamji – 단일 파일 런처 추가 및 임포트 경로 수정
- 2025-06-05 – kkamji – 프로젝트 및 CLI 이름 변경
- 2025-06-05 – kkamji – 초기 규칙/요구사항/히스토리 작성
