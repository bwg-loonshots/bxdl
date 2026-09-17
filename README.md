# BXDL

NIGO 기반 기업용 허가형 블록체인의 패키징·배포·운영 도구다. **Rust로 구현하며 macOS Apple Silicon에서 설치·운용 UX를 먼저 완성한다.** Linux 서버/systemd와 Docker/Compose는 후속 배포 대상으로 유지한다.

현재 구현은 개발용 패키지 조립·검증과 로컬 설정 검사 기반이다. 실제 installer·launchd·NIGO init/start/stop은 아직 구현하지 않았다. 엔진 JAR/JRE·고객 키·DB를 이 저장소에 동봉하지 않는다.

## 구현한 명령

| 명령 | 실제 동작 |
| --- | --- |
| `bxdl version --json` | Rust CLI identity·Mac 우선 target·구현 capability·엔진/서비스 미인수 상태 |
| `bxdl package build ...` | expected hash에 고정한 stage를 결정적인 development archive로 조립·선택 서명·자체 검증 |
| `bxdl package verify <archive> ...` | 외부 신뢰 key의 Ed25519 서명, 모든 payload hash/size/mode, 경로·형식·한도 검증 |
| `bxdl config validate --file <json>` | 제품 설정 schema·경로 기준·명시 endpoint 규칙 검사 |
| `bxdl preflight --config <json>` | 파일/디렉터리 metadata만 검사, 엔진·서비스 검사는 NOT_CHECKED |

`install/init/start/stop/status/logs/diagnose/upgrade/uninstall`은 exit 4와 CAPABILITY_NOT_IMPLEMENTED를 반환한다. 다음 구현의 대화형 `setup` 흐름은 [Mac 우선 설계](./design/2026-09-17-rust-macos-first.md)에 정의했다.

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
- [현재 구현·검증 상태](./docs/implementation-status.md)
- [패키지 입력·서명·형식](./docs/package-format.md)
- [CLI·설정·사전 검사](./docs/cli.md)
- [지원 후보와 도구·의존성](./docs/support-matrix.md)
- [전체 설계](./design/README.md), [제품 JSON 계약](./contracts/bxdl/README.md)

2026-09-16 Go 기반은 Rust로 대체했다. 과거 검증 기록만 results에 보존하며 현재 제품 빌드/테스트는 Cargo를 사용한다.
