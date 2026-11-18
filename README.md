# BXDL

BXDL은 **NIGO Protocol 기반 기업용 허가형 블록체인 노드를 패키징하고 설치·운용하는 제품**입니다. 엔진과 Java 실행 환경을 검증한 패키지로 구성하고, Rust로 작성한 CLI에서 노드 준비·초기화·서비스 제어·진단을 수행합니다.

관리 CLI는 노드 엔진과 별도로 배포하고 업데이트할 수 있습니다. 지원하는 조합에서는 기존 노드의 고정 실행기·JAR·JRE·데이터를 유지하면서 관리 CLI만 교체합니다.

현재 개발 버전이며 Linux 설치·운용 검증을 우선합니다. macOS의 지정 개발 패키지는 실제 설치·서비스·네트워크 검증을 거쳤습니다. Linux 노드의 실제 설치·운용 검증은 아직 완료되지 않았습니다. 자세한 범위는 [지원 환경](./docs/support-matrix.md)과 [구현 상태](./docs/implementation-status.md)에서 확인할 수 있습니다.

## 주요 기능

| 기능 | 내용 |
| --- | --- |
| 패키징·검증 | CLI·NIGO 엔진·JRE를 묶고 파일 목록·해시·크기·권한을 확인하며, 외부 신뢰 공개키로 서명을 검증합니다. |
| 순차 설치 준비 | `setup new/resume/list/show/edit`로 필요한 자료를 입력하고 준비 내용을 저장·조회·수정합니다. |
| 설치·초기화 | 검토한 계획을 `setup apply`로 설치하고 `init`으로 데이터를 명시적으로 초기화합니다. |
| 이름 기반 운용 | `nodes`, `start`, `status`, `stop`으로 등록한 노드를 이름으로 관리합니다. |
| 기록·진단 | `logs`, `diagnose`로 정제된 초기화·서비스 기록과 누락 사유를 확인합니다. |
| 관리 CLI 독립 배포 | `cli install/update/info/rollback`으로 CLI와 자동완성을 설치·전환·복귀합니다. |
| 자동화·도움말 | 지원 명령의 `--json`, 명령별 `--help`, zsh 자동완성을 제공합니다. |

## 지원 환경

| 환경 | 서비스 방식 | 현재 제공 범위 |
| --- | --- | --- |
| Linux x86_64 / glibc | systemd 시스템 서비스, 노드별 전용 계정 | Ubuntu 24.04 LTS 기준 개발용 설치·초기화·운용 구현. CLI 빌드·자동 검사 통과, 실제 노드·서비스 시험 대기 |
| macOS Apple Silicon | 사용자 LaunchAgent | 지정 RC.3·RC.4 패키지의 설치·운용과 관리 CLI 분리 지원. 지정 조합의 실물 검증 완료 |
| WSL2·Docker/Compose | 별도 후속 대상 | 현재 배포·운용 인수 범위에 포함되지 않음 |

Intel Mac·Linux ARM64·Windows native·musl은 현재 지원 대상이 아닙니다. 최소 macOS 버전과 정식 운영 지원 조건은 아직 확정되지 않았습니다.

## 시작하기

운영체제에 맞는 관리 CLI와 노드 공급물을 준비한 뒤 해당 가이드를 따릅니다. 설치 입력과 권한 준비 방식이 다르므로 다른 OS의 패키지를 섞어 사용하지 않습니다.

- **Linux 다운로드:** [GitHub Releases](https://github.com/bwg-loonshots/bxdl/releases)가 공식 다운로드 위치입니다. 첫 공개 시험판은 준비 중이며, 게시된 태그별 설치 명령과 필요한 공개키는 [curl 설치 안내](./docs/linux-release.md)에서 확인할 수 있습니다.
- **Linux 노드 운용:** [설치·운용 가이드](./docs/linux-operations.md). 일반 사용자로 준비하고, 고정된 관리자 실행기를 통해 승인한 시스템 설치를 수행합니다. 비공개 시험 패키지를 받았다면 동봉된 `START-HERE.md`와 `HANDOFF.json`부터 확인합니다.
- **macOS:** [순차 설치 준비](./docs/preparation.md)와 [관리 CLI 설치](./docs/management-cli.md). 노드는 로그인한 사용자의 LaunchAgent로 운용합니다.

기본 흐름은 **설치 준비 → 설치 승인 → 초기화 → 서비스 시작 → 상태·진단 → 정상 종료**입니다. 각 단계의 결과를 확인하고 다음 단계로 진행합니다.

```sh
bxdl setup new local
bxdl setup show local
bxdl setup apply local
```

설치가 완료되면 `bxdl init local --confirm-initialize`로 초기화하고 `bxdl start local`로 실행합니다. Linux 운용 명령에는 사전 `sudo -v` 인증이 필요합니다. 설치 문답과 초기화·시작은 각각 별도의 동작입니다.

| 할 일 | 명령 |
| --- | --- |
| 등록한 노드 보기 | `bxdl nodes` |
| 시작 후 준비 상태 기다리기 | `bxdl status local --wait` |
| 저장된 기록 보기 | `bxdl logs local` |
| 새 진단 파일 만들기 | `bxdl diagnose local --output ./local-diagnostic.json` |
| 정상 종료 요청·확인 | `bxdl stop local` |
| 명령 사용법 보기 | `bxdl <명령> --help` |

노드 이름·설치 준비는 사용자 관리 저장소에, 실제 노드의 실행기·설정·데이터는 해당 인스턴스 영역에 저장됩니다. 관리 CLI의 설치 위치는 이들과 별개입니다. [저장 위치와 노드 이름](./docs/nodes.md), [CLI 배포·업데이트](./docs/management-cli.md)를 참고하세요.

## 상태와 운영 범위

`nodes`는 저장한 목록을 보여주고, 현재 실행 상태는 `status`로 확인합니다. `READY`는 해당 노드의 로컬 준비 상태이며 네트워크 정족수나 거래 확정의 보장이 아닙니다. `logs/diagnose`는 저장 기록을 수집하며 누락이 있으면 부분 결과로 표시합니다.

현재 개발 패키지에는 자동 재시작·부팅 자동 실행·엔진 업그레이드·DB 마이그레이션·자동 제거가 포함되지 않습니다. 실패나 `UNKNOWN` 상태의 자료를 보존하고, 원인을 확인하기 전에 DB 삭제·재초기화를 하지 않습니다. 관리 CLI의 `rollback`은 엔진이나 DB 복구 기능과 별개입니다.

## 개발·패키지 제작

개발에는 Rust **1.86.0**, Python 3, 대상 플랫폼의 C 컴파일러·링커를 사용합니다. Rust toolchain과 Cargo 의존성은 저장소에서 고정합니다. 완성된 CLI 배포본을 실행하는 사용자에게 Rust/Cargo가 필요한 것은 아닙니다. 현재 비공개 Linux 시험은 native 빌드를 사전 단계로 포함합니다.

```sh
make check
make build
./bin/bxdl version --json
```

`make check`는 fmt·Clippy·Rust 테스트와 Python 도구 회귀 검사를 실행합니다. 실제 엔진·DB·서비스 인수는 별도입니다. `make build`의 `bin/bxdl`은 빌드 호스트용이며, Linux 후보 제작은 [네이티브 빌드·수신 가이드](./docs/linux-native-build.md)를 따릅니다.

패키지 제작자는 [JRE 준비](./docs/runtime.md), [패키지 형식·서명 검증](./docs/package-format.md), [엔진 lock·검사](./docs/engine.md)를 참고하세요. JAR·JRE·키·DB·생성된 배포 archive는 Git에 포함하지 않습니다.

## 문서

- [문서 안내](./docs/README.md): 설치·운용·제작·개발 문서의 시작점
- [CLI 사용법](./docs/cli.md): 명령, 입력, 출력과 오류 처리
- [지원 환경](./docs/support-matrix.md): 플랫폼·엔진 조합과 검증 경계
- [현재 구현 상태](./docs/implementation-status.md): 구현·인수 상태와 남은 작업
- [설계 안내](./design/README.md): 현재 설계와 상세 결정 자료
- [BXDL 데이터 계약](./contracts/bxdl/README.md): 설정·패키지·JSON 결과 schema
