# BXDL

NIGO 기반 기업용 허가형 블록체인의 패키징·배포·운영 도구다. **Rust로 구현하며 macOS Apple Silicon에서 설치·운용 UX를 먼저 완성한다.** Linux 서버/systemd와 Docker/Compose는 후속 배포 대상으로 유지한다.

현재 구현은 개발용 패키지 조립·검증, **Mac의 새 폴더 설치**, 재개 가능한 setup 설정 도우미, **제품·NIGO 설정의 결합 cold 검사**, **설치본·설정의 인스턴스 등록과 명시적 init/resume-init**이다. launchd·노드 start/status/stop은 후속이다. 엔진 JAR/JRE·고객 키·DB를 이 저장소에 동봉하지 않는다.

## 구현한 명령

| 명령 | 실제 동작 |
| --- | --- |
| `bxdl version --json` | Rust CLI identity·Mac 우선 target·구현 capability·엔진/서비스 미인수 상태 |
| `bxdl package build ...` | expected hash에 고정한 stage를 결정적인 development archive로 조립·선택 서명·자체 검증 |
| `bxdl package verify <archive> ...` | 외부 신뢰 key의 Ed25519 서명, 모든 payload hash/size/mode, 경로·형식·한도 검증 |
| `bxdl install <archive> --destination <new-dir> ...` | macOS arm64에서 검증한 payload를 새 폴더에 설치하고 완료 receipt 기록 |
| `bxdl engine inspect ...` | 신뢰 lock의 JAR·Java hash와 실제 engine-info 식별 정보 대조 |
| `bxdl engine preflight ... --config <node.json>` | NIGO native 설정의 cold 검사, INCOMPLETE 보존 |
| `bxdl config validate --file <json>` | 제품 설정 schema·경로 기준·명시 endpoint 규칙 검사 |
| `bxdl preflight --config <json>` | 파일/디렉터리 metadata만 검사, 엔진·서비스 검사는 NOT_CHECKED |
| `bxdl preflight --config <instance.json> --engine-config <node.json> ...` | 제품과 명시 QBFT 설정의 일치를 확인한 뒤 pinned 엔진 cold 검사 |
| `bxdl setup [--workspace <dir>]` | 설정 입력·수정·저장·재개와 로컬 검사, 새 제품 JSON 내보내기 |
| `bxdl instance register --instance <new-dir> ...` | 설치본·원본 archive·신뢰 key·제품/native 설정을 검증하고 private 등록 기록 생성 |
| `bxdl instance show --instance <dir>` | 저장된 초기화 상태와 현재 작업 잠금 관측. 노드 health 검사가 아님 |
| `bxdl preflight --instance <dir>` | 등록한 입력·설치본을 다시 검증하고 결합 cold 검사 |
| `bxdl init --instance <dir> --confirm-initialize` | 새/빈 데이터에 명시 초기화, 프로세스 종료·출력·report·엔진 journal 대조 |
| `bxdl resume-init --instance <dir> --confirm-resume` | 같은 identity의 INITIALIZING journal과 기존 ledger가 있는 미완료 시도만 명시 재개 |

`start/stop/status/logs/diagnose/upgrade/uninstall`은 exit 4와 CAPABILITY_NOT_IMPLEMENTED를 반환한다. setup 저장 성공은 설치·엔진 준비 완료가 아니며, init 성공도 실행 중 노드·서비스 준비 완료가 아니다.

## 설정 준비

Mac 터미널에서 `bxdl setup`을 실행하면 이름·경로·포트·기존 체인/키 자료의 파일 경로를 묻는다. 각 답을 저장하며 `:back`으로 수정하고 `:cancel`로 나갈 수 있다. 기존 초안은 `bxdl setup --resume`으로 이어간다. 기본 작업 폴더는 `$HOME/Library/Application Support/BXDL/setup`이며 새 폴더만 생성한다.

```bash
./bin/bxdl setup
./bin/bxdl setup --resume
```

입력 완료 후 `save`는 초안 저장, `export`는 확인 후 새 설정 JSON 저장이다. 참조 파일 누락이 있더라도 초안은 저장할 수 있으며 검사 결과에 실패가 남는다. 자동화의 `--from`/`--resume`·`--non-interactive`·`--json`과 경로·출력 규칙은 [setup 사용 가이드](./docs/setup.md)를 따른다.

설정과 native QBFT 자료를 준비하고 package를 설치한 뒤 [인스턴스 등록·초기화 가이드](./docs/instance.md)에 따라 한 번 등록한다. 이후에는 같은 `--instance` 제어 폴더를 사용한다. 등록 때 지정한 원본 archive·외부 신뢰 key·설정·키 자료는 계속 필요하다. UNKNOWN은 보존된 불명 상태이며 자동 init 재시도나 데이터 삭제로 해결하지 않는다.

## 개발과 실행

Rust **1.86.0**, `rust-toolchain.toml`과 `Cargo.lock`을 사용한다. 첫 개발 빌드에는 compiler/linker와 Cargo 의존성 다운로드가 필요하다. 배포된 CLI를 실행하는 사용자는 Rust/Cargo를 설치할 필요가 없다.

```bash
make check
make build
./bin/bxdl version --json
./bin/bxdl config validate --file config/examples/instance.development.json --json
./bin/bxdl preflight --config config/examples/instance.development.json --json
make build-macos
```

`make check`는 fmt·Clippy·단위/통합 테스트다. 예시 설정은 파일 참조만 담고 실제 chain/key/DB를 제공하지 않으므로 preflight 실패가 정상이다. schema 통과는 엔진 기동 가능 여부가 아니다. macOS arm64 산출물은 `dist/bxdl-darwin-arm64`이며 `bin/bxdl`은 빌드 호스트의 실행파일이다.

Linux builder에서는 `make build-linux`를 사용한다. Mac에서 Linux target type-check는 `make check-linux`로 가능하지만 먼저 해당 Rust target이 필요하다. 실제 Linux linking에는 적절한 Linux linker/sysroot가 필요하며 type-check를 실행 인수로 취급하지 않는다. 의존성을 미리 확보했다면 `cargo ... --locked --offline`을 사용할 수 있다.

## 패키지 검증

첫 설치 때도 검증 전 package 안의 실행파일을 root로 실행하지 않는다. 신뢰 key는 archive 밖에서 공급한다.

```bash
./bin/bxdl package verify ./bxdl-development.tar.gz --public-key ./trusted-release-public.pem --json
# 서명 없는 개발 fixture에만 명시 opt-in
./bin/bxdl package verify ./fixture.tar.gz --allow-unsigned-development --json
```

현재 manifest v1은 `darwin/arm64`(우선)와 기존 `linux/amd64/glibc` 개발 profile을 받는다. `channel=development`, `engine.contractStatus=proposed`만 허용한다. 내용·서명 검증은 JAR/JRE 실행 가능성·공식 공급 인증·Apple code signing·OS 서비스 지원을 보증하지 않는다.

## 문서

- [Rust·macOS 우선 결정과 설치 UX](./design/2026-09-17-rust-macos-first.md)
- [제품·엔진 설정 연결 설계](./design/2026-09-18-product-engine-preflight.md)
- [인스턴스 등록·초기화 설계](./design/2026-09-18-instance-initialization.md)
- [현재 구현·검증 상태](./docs/implementation-status.md)
- [설정 도우미·초안 저장과 재개](./docs/setup.md)
- [패키지 입력·서명·형식](./docs/package-format.md)
- [Mac 패키지 설치와 실패 처리](./docs/install.md)
- [인스턴스 등록·명시 초기화와 중단 처리](./docs/instance.md)
- [엔진 lock·개발 후보 cold 검사](./docs/engine.md)
- [CLI·설정·사전 검사](./docs/cli.md)
- [지원 후보와 도구·의존성](./docs/support-matrix.md)
- [전체 설계](./design/README.md), [제품 JSON 계약](./contracts/bxdl/README.md)

2026-09-16 Go 기반은 Rust로 대체했다. 과거 검증 기록만 results에 보존하며 현재 제품 빌드/테스트는 Cargo를 사용한다.
