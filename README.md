# BXDL

NIGO 기반 기업용 허가형 블록체인의 패키징·배포·운영 도구다. **Rust로 구현하며 macOS Apple Silicon에서 설치·운용 UX를 먼저 완성한다.** Linux 서버/systemd와 Docker/Compose는 후속 배포 대상으로 유지한다.

개선 우선순위를 **직접 CLI 중심**으로 변경했다. `bxdl console`의 추가 개발은 중단하고 기존 구현은 보존한다. `bxdl nodes`와 `bxdl nodes add <이름> --instance <경로>`로 console 없이 목록 조회와 기존 노드 이름 연결을 제공한다. 이름 기반 운용을 연결하고 관리 CLI가 기존 RC.3 패키지 실행기에 작업을 위임하도록 분리했다. 순차 설치 준비는 후속이다. 현재 `setup new/resume`의 번호 메뉴는 교체 예정이며 아직 새 순차 흐름은 제공하지 않는다. [현재 작업 계획](./design/2026-10-22-integrated-operations-ux-implementation-plan.md)을 따른다.

현재 구현은 개발용 패키지 조립·검증, **Mac의 새 폴더 설치**, 초안 모드와 **명시 설치·등록·초기화·시작을 연결한 setup 설치 도우미**, **제품·NIGO 설정의 결합 cold 검사**, **인스턴스 등록과 명시적 init/resume-init**, **Mac 사용자 LaunchAgent의 start/status/stop**, **오프라인 logs/diagnose**다. 새 setup CLI와 동일한 CLI를 담은 개발용 package로 취소·재개, 설치·초기화, 단일 노드 시작·정상 종료·같은 데이터 재시작을 확인했다. [검증 범위와 남은 인수](./results/2026-09-27-setup-install-flow.md)를 따르며 전체 G1-M은 후속이다. 엔진 JAR/JRE·고객 키·DB를 이 저장소에 동봉하지 않는다.

2026-10-19에는 [NIGO RC1](./contracts/nigo/engine-rc1-2026-10-19/README.md)을 원격 수신하고 등록 모드 안내를 추가했다.
실제 RC1·동일 CLI를 담은 서명 개발 package에서 전체 14문답·설치·초기화·정상 종료·같은 DB 재시작을 확인했다.
[RC1 검증 결과](./results/2026-10-19-engine-rc1-adoption.md)를 보존하며 Temurin21.0.12.1+1 full JRE 고정과 NIGO J1~J3 검증 회신을 수용했다.
2026-10-20 [동일 package의 4-validator 시험](./results/2026-10-20-macos-qbft-package.md)에서 거래 확정·catch-up·잘못된 pin 거부를 확인했다. 당시 장시간 정족수 부족 이후 합의 복구는 INCOMPLETE로 보존했다.
2026-10-21에는 [RC.2를 선택](./contracts/nigo/engine-rc2-2026-10-21/README.md)하고 새 Mac 패키지의 **4-validator·정족수 상실 후 실시간 복구·동일 네 DB 전체 재시작 복구·wrong-pin 격리**를 통과했다. [RC.2 실제 결과](./results/2026-10-21-macos-rc2-package.md)를 따른다. 기존 설치 upgrade와 전체 G1-M·정식 고객 배포는 남아 있다.

같은 RC.2 archive의 [전체 설치 UX 재검증](./results/2026-10-21-macos-rc2-setup.md)에서는 지정 시험 경로의 20개 문답·중단/재개·운용 사례가 통과했다. 원본 자동 집계는 INCOMPLETE로 보존하며 후속 독립 검사에서 잔존 프로세스·리스너가 없음을 확인했다. 별도 Documents 참조 실행의 시작 전 지연은 미해결이다.

2026-10-22 후속 변경은 새 등록의 archive·engine lock·선택 신뢰 공개키를 private `trust/`에 보관하고, 시작 worker가 마지막으로 진입한 검사 단계를 status/setup/diagnose에 표시한다. 기존 V1 등록은 자동 변환하지 않는다. [등록 자료 보관과 시작 진단 설계](./design/2026-10-22-managed-supply-and-start-diagnostics.md)를 따르며 **새 RC.2 시험 package의 지정 로컬 13사례 PASS**이며, 원본 공급 경로 부재 상태의 재개·init/start/stop·동일 DB 재시작과 진단을 확인했다. [검증 결과](./results/2026-10-22-macos-managed-supply.md)에 정확한 범위를 기록했다. 이 변경으로 위 지연의 원인이나 모든 자료 경로의 접근 문제가 해결됐다고 주장하지 않는다.

별도로 [NIGO RC.3 수신](./contracts/nigo/engine-rc3-2026-10-22/README.md)의 archive22파일·계약·선정 Temurin 실제 `engine-info` 대조를 통과했다. 이후 **RC.3를 새 Mac 개발 패키지 후보로 명시 선택**하고 [전용 engine lock](./packaging/engine-lock.rc3-temurin-development.json)·[package spec](./packaging/package-spec.rc3-temurin.example.json)을 추가했다. [재인수 설계](./design/2026-10-22-rc3-package-reacceptance.md)에 따른 같은 새 서명 archive의 **4-validator·두 정족수 복구·wrong-pin과 원본 공급 경로 부재 상태의 setup 13사례가 PASS**다. [네트워크·복구 결과](./results/2026-10-22-macos-rc3-package.md)와 [setup 결과](./results/2026-10-22-macos-rc3-setup.md)에 실제 identity·정상 종료·독립 정리 검증을 기록했다.

앞선 setup13사례는 제품 설정을 가져온 범위다. 이후 [NIGO PR #230](https://github.com/bwg-loonshots/nigo-protocol/pull/230)으로 정식 인수를 회신·머지하고, **같은 archive의 전체14문답·수정·취소/EOF·대기 중 HUP·원본 공급 경로 부재 운용21사례와 독립 검사8항목을 PASS**했다. [전체 설치 UX 결과](./results/2026-10-22-macos-rc3-full-setup.md)에 초기화1회·서비스2회·정상 stop2회와 잔존 PID/job/LISTEN 부재를 기록했다. 대기 중 HUP을 mutation 중단이나 Mac 로그아웃 검증으로 확대하지 않는다.

**offline Storage A7은 NOT_RUN**, 기존 설치 upgrade와 전체 G1-M은 후속이다. RC.2 원본 INCOMPLETE·경로 원인 미확정과 RC.3 수신/앞선 시험/NIGO 회신의 당시 미선택·NOT_RUN은 각 시점의 이력으로 보존한다.

관리 CLI는 노드 패키지 밖에서 독립적으로 업데이트할 수 있다. 현재 지원 대상은 검증한 기존 RC.3 패키지이며 설치본의 CLI·NIGO JAR·JRE를 교체하지 않는다. 실행기 자체를 변경할 때는 새 패키지 인수가 필요하다. [분리 설계](./design/2026-10-23-independent-management-cli.md)와 [운용 가이드](./docs/management-cli.md)를 따른다. [독립 관리 CLI 인수](./results/2026-10-23-independent-management-cli.md)의 직접 운용16사례·설치 도우미 PTY8사례, payload/worker 불변과 정상 종료 독립 대조가 PASS다.

## 구현한 명령

| 명령 | 실제 동작 |
| --- | --- |
| `bxdl nodes [--home <dir>] [--json]` | 저장한 노드 목록·이름·저장 당시 상태/시각 조회. 현재 실행 상태는 미조회 ([가이드](./docs/nodes.md)) |
| `bxdl nodes add <이름> --instance <control> [--home <dir>] [--json]` | 기존 등록에 고유 이름 연결. 추가 문답 없이 목록만 저장하며 노드·DB·서비스는 변경하지 않음 |
| `bxdl status/start/stop/logs <이름>`, `bxdl diagnose <이름> --output <새.json>` | 같은 관리 목록의 이름을 해석해 노드 운용. `--home` 지원, 기존 `--instance` 경로도 유지 |
| `bxdl setup new <이름>` / `bxdl setup resume <이름>` | 같은 이름으로 자료·제품·network/native 설정 준비·재개. 실제 설치는 후속 ([가이드](./docs/preparation.md)) |
| `bxdl console [--home <dir>] [--instance <control>]` | **추가 개발 중단·기존 구현 보존.** 홈·내 목록·기존 등록/세션의 저장 기록 조회. 승인한 목록 저장/갱신/숨기기와 새 설치 자료 준비 ([가이드](./docs/console.md)) |
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
| `bxdl config fingerprint --certificate <leaf.pem>` | 공개 인증서의 NIGO MTLS pin 계산 |
| `bxdl setup --network --workspace <new-session> --from <instance.json> --network-file <network.json>` | RC.1~RC.3 공통 native 계약의 설정 준비 및 재개 |
| `bxdl setup --install --workspace <new-session>` | Mac 터미널에서 자료·계획을 저장하고 설치/등록·초기화·시작을 각각 명시 선택. `--resume`으로 같은 세션 재검증 |
| `bxdl instance register --instance <new-dir> ...` | 설치본·원본 archive·신뢰 key·제품/native 설정을 검증하고, 새 V2 등록의 공급 자료 보관본과 private 기록 생성 |
| `bxdl instance show --instance <dir>` | 저장된 초기화 상태와 현재 작업 잠금 관측. 노드 health 검사가 아님 |
| `bxdl preflight --instance <dir>` | 등록한 입력·설치본을 다시 검증하고 결합 cold 검사 |
| `bxdl init --instance <dir> --confirm-initialize` | 새/빈 데이터에 명시 초기화, 프로세스 종료·출력·report·엔진 journal 대조 |
| `bxdl resume-init --instance <dir> --confirm-resume` | 같은 identity의 INITIALIZING journal과 기존 ledger가 있는 미완료 시도만 명시 재개 |
| `bxdl start --instance <dir>` | 초기화한 인스턴스를 시도별 LaunchAgent로 명시 시작. gate를 통과한 worker가 같은 PID로 Java 실행 |
| `bxdl status --instance <dir>` | 해당 시도의 launchd·report·로컬 HTTP와 마지막 시작 검사 단계 조회. 전역 quorum 검사가 아님 |
| `bxdl status --instance <dir> --wait` | 현재 시도의 준비 상태를 기본120초 읽기 관측. 기한 만료·실제 실패·시도 변경 구분 |
| `bxdl stop --instance <dir>` | 소유 job에 TERM 요청 후 엔진 정상 종료 보고·프로세스 부재·잠금 해제를 함께 확인 |
| `bxdl logs --instance <dir>` | 최신 초기화·서비스 시도의 정제한 저장 이벤트와 누락 사유를 제한해 조회 |
| `bxdl diagnose --instance <dir> --output <new.json>` | 같은 오프라인 기록을 새 JSON 지원 보고서로 저장. 현재 health 검증은 수행하지 않음 |

`upgrade/uninstall`은 exit 4와 CAPABILITY_NOT_IMPLEMENTED를 반환한다. 기본 setup 저장 성공은 초안 저장이며 설치 모드의 `finish`도 현재 단계까지만 저장한다. init 성공은 실행 중 노드·서비스 준비 완료가 아니다. logs/diagnose는 raw 로그·비밀·경로·hash를 내보내지 않고 누락은 partial로 표시한다. 보고서 생성 성공을 노드 안전·정상 판정으로 해석하지 않는다. [사용법과 한도](./docs/cli.md#오프라인-기록과-지원-보고서)를 따른다.

지원하는 기존 RC.3는 외부 관리 CLI로 start를 호출하면 인증된 package의 고정 실행기에 위임한다. 다른 package는 기존 동일 bytes 조건을 적용한다. 관리 CLI는 설치본 밖에서 교체하며 package 내부 CLI는 덮어쓰지 않는다. 로그인된 Mac 사용자 세션에서 수동 시작하며 로그인 자동 시작·자동 재시작은 제공하지 않는다. 정상 종료가 검증된 뒤에만 다음 명시 start를 허용한다. 실패·UNKNOWN 복구와 보존된 snapshot/private 로그의 자동 정리는 후속이다.

## 설치 도우미와 설정 준비

Mac arm64에서 새 설치를 진행하려면 별도 설치 세션을 지정한다. 검증된 기존 RC.3는 독립 관리 CLI로 설치·운용할 수 있으며, 다른 package는 공급된 동일 CLI 조건을 적용한다. 부모 폴더는 미리 있어야 하며 자동 생성하지 않는다.

```bash
bxdl setup --install --workspace /absolute/existing-parent/new-session
bxdl setup --install --workspace /absolute/existing-parent/new-session --resume
```

archive·외부 신뢰 공개키(또는 명시 unsigned-development)·engine lock·native node.json과 제품 설정을 준비한다. `plan` 뒤 `apply`와 확인은 설정·설치·등록까지 진행하고, `init`과 `start`는 각각 별도 확인이 필요하다. `finish`로 원하는 단계에서 나가거나 `:cancel`로 저장 후 중단할 수 있다. 새 설치 모드는 TTY 전용이며 `--json`·`--output`·`--non-interactive`를 받지 않는다. 기존 DB 업그레이드·키 발급은 제공하지 않는다. Native 파일은 별도 [네트워크 준비 도우미](./docs/network.md)로 만들 수 있다. [설치 흐름과 재개 조건](./docs/setup.md#설치-모드-패키지부터-시작까지)을 따른다.

Mac 터미널에서 `bxdl setup`을 실행하면 이름·경로·포트·기존 체인/키 자료의 파일 경로를 묻는다. 각 답을 저장하며 `:back`으로 수정하고 `:cancel`로 나갈 수 있다. 기존 초안은 `bxdl setup --resume`으로 이어간다. 기본 작업 폴더는 `$HOME/Library/Application Support/BXDL/setup`이며 새 폴더만 생성한다.

```bash
./bin/bxdl setup
./bin/bxdl setup --resume
```

입력 완료 후 `save`는 초안 저장, `export`는 확인 후 새 설정 JSON 저장이다. 참조 파일 누락이 있더라도 초안은 저장할 수 있으며 검사 결과에 실패가 남는다. 자동화의 `--from`/`--resume`·`--non-interactive`·`--json`과 경로·출력 규칙은 [setup 사용 가이드](./docs/setup.md)를 따른다.

개별 명령을 사용할 때는 설정과 native QBFT 자료를 준비하고 package를 설치한 뒤 [인스턴스 등록·초기화 가이드](./docs/instance.md)에 따라 한 번 등록한다. 이후에는 같은 `--instance` 제어 폴더를 사용한다. 설치 도우미 작업 폴더와 운용 설정·키 참조는 계속 필요하다. 새 V2 등록 완료 후 archive·engine lock·선택 신뢰 공개키는 `trust/` 보관본으로 검증하며, 기존 V1 등록은 원본 자료도 계속 필요하다. UNKNOWN은 보존된 불명 상태이며 자동 init 재시도나 데이터 삭제로 해결하지 않는다.

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

`make check`는 fmt·Clippy·Rust 단위/통합 테스트와 Python 3 표준 라이브러리 기반의 오프라인 인수 도구 회귀다. `make test-acceptance`는 이 Python 검사만 실행하며 노드를 시작하지 않는다. 예시 설정은 파일 참조만 담고 실제 chain/key/DB를 제공하지 않으므로 preflight 실패가 정상이다. schema 통과는 엔진 기동 가능 여부가 아니다. macOS arm64 산출물은 `dist/bxdl-darwin-arm64`이며 `bin/bxdl`은 빌드 호스트의 실행파일이다.

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
- [Mac LaunchAgent의 시작·관측·종료 설계](./design/2026-09-18-macos-launchagent.md)
- [setup 설치 흐름·소유 결과·재개 설계](./design/2026-09-27-setup-install-flow.md)
- [등록 자료 보관과 시작 진단 설계](./design/2026-10-22-managed-supply-and-start-diagnostics.md)
- [원본 공급 경로 부재 상태의 Mac 검증](./results/2026-10-22-macos-managed-supply.md)
- [RC.3 수신 검증과 당시 범위](./contracts/nigo/engine-rc3-2026-10-22/README.md)
- [RC.3 선택과 Mac 패키지 재인수](./design/2026-10-22-rc3-package-reacceptance.md)
- [RC.3 전체14문답·취소/EOF·대기 중 HUP·재개 검증](./results/2026-10-22-macos-rc3-full-setup.md)
- [현재 구현·검증 상태](./docs/implementation-status.md)
- [설치 도우미·설정 초안 저장과 재개](./docs/setup.md)
- [패키지 입력·서명·형식](./docs/package-format.md)
- [Mac 패키지 설치와 실패 처리](./docs/install.md)
- [인스턴스 등록·초기화·서비스 제어와 중단 처리](./docs/instance.md)
- [엔진 lock·개발 후보 cold 검사](./docs/engine.md)
- [CLI·설정·사전 검사](./docs/cli.md)
- [지원 후보와 도구·의존성](./docs/support-matrix.md)
- [전체 설계](./design/README.md), [제품 JSON 계약](./contracts/bxdl/README.md)

2026-09-16 Go 기반은 Rust로 대체했다. 과거 검증 기록만 results에 보존하며 현재 제품 빌드/테스트는 Cargo를 사용한다.

선정 Temurin21과 제작자용 `runtime stage`는 [JRE 준비](docs/runtime.md)를 참고한다.
[실제 Temurin 패키지 검증](results/2026-10-19-temurin-runtime.md): 자동 검사290개·설치/초기화/정상 종료/동일 DB 재시작 PASS.
[최종 잠금 수정 재검증](results/2026-10-19-workflow-lock-release.md): 자동 검사292개 통과. 새 패키지의 재시작 관측은 UNKNOWN으로 자동 시험 INCOMPLETE이며, 동일 실행의 후속 READY·정상 종료·재개는 별도 확인했다.

[NIGO J1~J3 회신 수신](contracts/nigo/jre-feedback-2026-10-19/README.md): 지정 RC1/Temurin 공급자9사례 PASS를 수용했다. [시작 관측 UX](design/2026-10-19-start-observation-ux.md)는 같은 실행의 대기와 기한 종료 안내를 제공한다. 실제 BXDL 제품 인수와 공급망 잔여는 별도로 관리한다.

[시작 관측 UX 검증](results/2026-10-19-start-observation-ux.md): 자동 검사312개, 실제 package의 짧은 상태 관측 만료·같은 시도 READY·정상 종료·재개 PASS. 전역 합의는 NOT_CHECKED다.
